//! STEP (ISO 10303-21) import. Port of `web/step.js` (+ `web/stepworker.js`).
//!
//! The web reads STEP with OpenCascade 7.6.1 compiled to WASM (occt-import-js 0.0.23). This crate runs
//! the same kernel natively: `cpp/step_shim.cpp` is occt-import-js's importer (`importer-xcaf.cpp`,
//! `importer-utils.cpp`, `js-interface.cpp`) with the JS plumbing replaced by a C API, built against a
//! static OpenCascade at the same commit (d2abb6d8, "Update version up to 7.6.1"; tools/build-occt.sh),
//! and called with the same tessellation settings ([`STEP_LINEAR_DEFLECTION`], [`STEP_ANGULAR_DEFLECTION`]).
//! [`read_step`] is then `stepObjects`: the assembly tree picks the objects, their meshes become flat
//! triangle soups.
//!
//! Without OpenCascade at build time (`--cfg no_occt`, see build.rs) [`read_step`] returns
//! [`NOT_AVAILABLE`] and [`is_step`] still works. The `occt` feature (on by default) is what links OpenCascade: the
//! app's engine (`sf-ffi`) turns it off and never runs the kernel in the app's process. A malformed file can crash
//! OpenCascade (X-01), so the app reads STEP in its `StepReader` XPC service (`crates/sf-step-xpc`), which sends the
//! kernel's result back as bytes ([`wire`]), and only [`step_objects`] runs in the app.

/// Port of `STEP_PARAMS.linearDeflection` (web/step.js:29-34): 0.01 mm chord error, absolute.
pub const STEP_LINEAR_DEFLECTION: f64 = 0.01;
/// Port of `STEP_PARAMS.angularDeflection` (web/step.js:29-34): 0.1 rad.
pub const STEP_ANGULAR_DEFLECTION: f64 = 0.1;

/// What [`read_step`] says when this build has no OpenCascade.
pub const NOT_AVAILABLE: &str = "STEP import is not available in this build (OpenCascade was not found when it was built; run `just occt`)";

/// `\s` of a JS regular expression.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Port of `isStep` (web/step.js:44-47): a STEP file from its content, "ISO-10303-21;" after an optional
/// BOM / leading whitespace. `TextDecoder` drops one leading BOM and turns invalid UTF-8 into U+FFFD.
pub fn is_step(buffer: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&buffer[..buffer.len().min(64)]);
    let head = head.strip_prefix('\u{feff}').unwrap_or(&head);
    let rest = head.trim_start_matches(is_js_space);
    let Some(rest) = rest.strip_prefix("ISO-10303-21") else {
        return false;
    };
    rest.trim_start_matches(is_js_space).starts_with(';')
}

/// One entry of the assembly tree the kernel writes (`root` of the occt-import-js result).
#[derive(Clone, Debug, PartialEq)]
pub struct RawNode {
    pub name: String,
    /// indices into [`RawResult::meshes`]
    pub meshes: Vec<usize>,
    /// indices into [`RawResult::nodes`]
    pub children: Vec<usize>,
}

/// One kernel mesh: stepworker.js has stored `position` in a `Float32Array` and `index` in a `Uint32Array`.
#[derive(Clone, Debug, PartialEq)]
pub struct RawMesh {
    pub name: String,
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
}

/// The kernel's result as the worker posts it (`success: true`): node 0 is `root`.
#[derive(Clone, Debug, PartialEq)]
pub struct RawResult {
    pub nodes: Vec<RawNode>,
    pub meshes: Vec<RawMesh>,
}

/// The bounding box of a soup (`bbox`, web/step.js:49-57).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bbox {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub size: [f64; 3],
}

/// One pickable object (`stepObjects`' entries, web/step.js:97-103).
#[derive(Clone, Debug, PartialEq)]
pub struct StepObject {
    pub name: String,
    /// the flat soup, 9 numbers per triangle, f32-rounded (a `Float32Array`)
    pub positions: Vec<f64>,
    pub tris: usize,
    pub meshes: usize,
    pub bbox: Bbox,
}

pub mod wire;

#[cfg(not(no_occt))]
#[allow(unsafe_code)]
mod ffi;

/// Run the kernel (`occt.ReadStepFile(bytes, STEP_PARAMS)` in the worker). `None` is `success: false`.
#[cfg(not(no_occt))]
pub fn read_raw(bytes: &[u8]) -> Result<Option<RawResult>, String> {
    Ok(ffi::read(
        bytes,
        STEP_LINEAR_DEFLECTION,
        STEP_ANGULAR_DEFLECTION,
    ))
}

#[cfg(no_occt)]
pub fn read_raw(_bytes: &[u8]) -> Result<Option<RawResult>, String> {
    Err(NOT_AVAILABLE.into())
}

/// Whether this build can read STEP at all.
pub fn available() -> bool {
    cfg!(not(no_occt))
}

/// `bbox` (web/step.js:49-57).
fn bbox(positions: &[f64]) -> Bbox {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in positions.as_chunks::<3>().0 {
        for k in 0..3 {
            if v[k] < lo[k] {
                lo[k] = v[k];
            }
            if v[k] > hi[k] {
                hi[k] = v[k];
            }
        }
    }
    Bbox {
        lo,
        hi,
        size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
    }
}

/// What [`step_objects`] says for the kernel's `success: false`, and for a result that is not a tree over its own nodes
/// and meshes (X-12; the shim never writes one, but the app gets the result from its STEP reader process).
pub const UNREADABLE: &str = "the CAD kernel could not read this STEP file";

/// `subtreeMeshes` (web/step.js:60-64): every mesh index under a node of the assembly tree, the node's own first, then
/// each child's in order. Iterative, with `budget` counting the nodes visited: a tree visits each node once at most, so
/// running out is a cycle (the JS would recurse until the stack overflows). `None` is a malformed tree.
fn subtree_meshes(
    r: &RawResult,
    node: usize,
    out: &mut Vec<usize>,
    budget: &mut usize,
) -> Option<()> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        *budget = budget.checked_sub(1)?;
        let n = r.nodes.get(n)?;
        out.extend_from_slice(&n.meshes);
        stack.extend(n.children.iter().rev());
    }
    Some(())
}

/// Port of `stepObjects` (web/step.js:79-120): the kernel's result -> pickable objects. A `None` result is
/// the kernel's `success: false`.
///
/// What counts as "one object" comes from the assembly tree: walk down through single-child wrapper nodes
/// (the file root, a lone top-level assembly) to the first level that actually branches, and make each branch
/// one object with all its meshes merged.
///
/// Every index is checked (X-12): a node, child, mesh or vertex index out of range, or a cycle, is [`UNREADABLE`].
pub fn step_objects(result: Option<&RawResult>) -> Result<Vec<StepObject>, String> {
    let Some(result) = result else {
        return Err(UNREADABLE.into());
    };
    step_objects_checked(result).ok_or_else(|| UNREADABLE.to_string())?
}

/// [`step_objects`] of a result; `None` when the result is malformed.
fn step_objects_checked(result: &RawResult) -> Option<Result<Vec<StepObject>, String>> {
    let at = |i: usize| result.nodes.get(i);
    let mut node = 0usize;
    // a wrapper chain longer than the node count is a cycle
    let mut hops = 0usize;
    while at(node)?.meshes.is_empty() && at(node)?.children.len() == 1 {
        node = at(node)?.children[0];
        hops += 1;
        if hops > result.nodes.len() {
            return None;
        }
    }
    let groups: Vec<usize> = if !at(node)?.meshes.is_empty() || at(node)?.children.is_empty() {
        vec![node] // leaf body, or meshes at this level
    } else {
        at(node)?.children.clone()
    };

    let mut budget = result.nodes.len();
    let mut objects: Vec<StepObject> = Vec::new();
    for g in groups {
        let mut ids = Vec::new();
        subtree_meshes(result, g, &mut ids, &mut budget)?;
        let mut tris = 0usize;
        for &i in &ids {
            tris += result.meshes.get(i)?.indices.len() / 3;
        }
        if tris == 0 {
            continue;
        }

        // Indexed -> flat soup, [x,y,z] x 3 per triangle: the STLLoader layout.
        let mut positions = Vec::with_capacity(tris * 9);
        for &i in &ids {
            let p = &result.meshes[i].positions;
            for &idx in &result.meshes[i].indices {
                let v = idx as usize * 3;
                let xyz = p.get(v..v + 3)?;
                positions.push(xyz[0] as f64);
                positions.push(xyz[1] as f64);
                positions.push(xyz[2] as f64);
            }
        }
        let name = if !result.nodes[g].name.is_empty() {
            result.nodes[g].name.clone()
        } else if !result.meshes[ids[0]].name.is_empty() {
            result.meshes[ids[0]].name.clone()
        } else {
            format!("Body {}", objects.len() + 1)
        };
        let b = bbox(&positions);
        objects.push(StepObject {
            name,
            positions,
            tris,
            meshes: ids.len(),
            bbox: b,
        });
    }

    if objects.is_empty() {
        return Some(Err("this STEP file contains no solid geometry".into()));
    }
    Some(Ok(objects))
}

/// Port of `readStep` (web/step.js:140-152): a STEP file's bytes -> its pickable objects.
pub fn read_step(bytes: &[u8]) -> Result<Vec<StepObject>, String> {
    let raw = read_raw(bytes)?;
    step_objects(raw.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_step_sniffs_the_part_21_magic() {
        assert!(is_step(b"ISO-10303-21;\nHEADER;"));
        assert!(is_step("\u{feff}\n  ISO-10303-21;\nHEADER;".as_bytes()));
        assert!(is_step(b"  ISO-10303-21 ;"));
        assert!(!is_step(b"solid cube\n facet normal 0 0 1"));
        assert!(!is_step(&[0x50, 0x4b, 3, 4, 0, 0]));
        assert!(!is_step(b""));
        assert!(!is_step(b"ISO-10303-21"));
    }

    fn node(name: &str, meshes: &[usize], children: &[usize]) -> RawNode {
        RawNode {
            name: name.into(),
            meshes: meshes.into(),
            children: children.into(),
        }
    }
    fn mesh(name: &str) -> RawMesh {
        RawMesh {
            name: name.into(),
            positions: vec![0., 0., 0., 1., 0., 0., 0., 1., 0.],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn step_objects_walks_through_wrappers_to_the_first_branching_level() {
        // root -> asm -> {a(mesh 0), b -> {b1(mesh 1), b2(mesh 2)}}
        let r = RawResult {
            nodes: vec![
                node("", &[], &[1]),
                node("asm", &[], &[2, 3]),
                node("a", &[0], &[]),
                node("", &[], &[4, 5]),
                node("b1", &[1], &[]),
                node("b2", &[2], &[]),
            ],
            meshes: vec![mesh("m0"), mesh("m1"), mesh("")],
        };
        let o = step_objects(Some(&r)).unwrap();
        assert_eq!(o.len(), 2);
        assert_eq!((o[0].name.as_str(), o[0].meshes, o[0].tris), ("a", 1, 1));
        // the second group has no name: the first mesh's; two meshes merged
        assert_eq!((o[1].name.as_str(), o[1].meshes, o[1].tris), ("m1", 2, 2));
        assert_eq!(o[1].positions.len(), 18);
    }

    /// X-12: a kernel result that is not a tree over its own nodes and meshes (none of these come out of the shim,
    /// but the app now gets the result from another process) is an error, never a panic or a hang.
    #[test]
    fn step_objects_refuses_a_malformed_result() {
        let unreadable = UNREADABLE;
        let mut bad_vertex = mesh("");
        bad_vertex.indices = vec![0, 1, 3];
        let bad = [
            // no root
            RawResult {
                nodes: vec![],
                meshes: vec![],
            },
            // a child that is not a node, below a wrapper and at the branching level
            RawResult {
                nodes: vec![node("", &[], &[5])],
                meshes: vec![],
            },
            RawResult {
                nodes: vec![node("", &[], &[1, 9]), node("a", &[0], &[])],
                meshes: vec![mesh("")],
            },
            // a mesh that is not a mesh
            RawResult {
                nodes: vec![node("", &[3], &[])],
                meshes: vec![mesh("")],
            },
            // an index past the mesh's vertices
            RawResult {
                nodes: vec![node("", &[0], &[])],
                meshes: vec![bad_vertex],
            },
        ];
        for r in &bad {
            assert_eq!(step_objects(Some(r)).unwrap_err(), unreadable, "{r:?}");
        }
    }

    /// X-12: a cycle in the tree (through the wrappers, or below the branching level) ends with an error.
    #[test]
    fn step_objects_refuses_a_cycle() {
        let unreadable = UNREADABLE;
        let wrappers = RawResult {
            nodes: vec![node("", &[], &[1]), node("", &[], &[0])],
            meshes: vec![],
        };
        let below = RawResult {
            nodes: vec![
                node("", &[], &[1, 2]),
                node("a", &[0], &[1]),
                node("b", &[0], &[]),
            ],
            meshes: vec![mesh("")],
        };
        for r in [&wrappers, &below] {
            assert_eq!(step_objects(Some(r)).unwrap_err(), unreadable, "{r:?}");
        }
    }

    #[test]
    fn step_objects_names_and_errors() {
        let r = RawResult {
            nodes: vec![node("", &[0], &[])],
            meshes: vec![mesh("")],
        };
        assert_eq!(step_objects(Some(&r)).unwrap()[0].name, "Body 1");
        assert_eq!(
            step_objects(None).unwrap_err(),
            "the CAD kernel could not read this STEP file"
        );
        let empty = RawResult {
            nodes: vec![node("", &[], &[])],
            meshes: vec![],
        };
        assert_eq!(
            step_objects(Some(&empty)).unwrap_err(),
            "this STEP file contains no solid geometry"
        );
    }
}
