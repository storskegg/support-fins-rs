//! Port of `web/threemf.js`: the 3MF writer (core spec, 2015/02 namespace) and reader.
//!
//! Why 3MF alongside the STL: it carries the two things a raw STL cannot, both of which
//! matter to this tool specifically --
//!   1. UNITS. STL is unitless, so a slicer has to guess millimeters; a mis-guess is the
//!      classic "my part imported at 1/25 scale" bug. 3MF states mm.
//!   2. SEPARATE BODIES, two ways (`separate`):
//!      - LOCKED (default here; the CLI and plugins): the part and the fins are two meshes
//!        assembled by <components> into ONE build item, so they stay locked in place. But a
//!        slicer unions the parts of one object, so it runs one perimeter through part and
//!        tine -- the tine welds (GitHub #38).
//!      - SEPARATE (the site's default 3MF): two build items, two objects, the way Clough42
//!        splits his support ("Split to objects"). Each keeps its own perimeter and a tine only
//!        touches the part: a cleaner release. Both items carry the same transform, so they
//!        open in register. With the interface crest on (GitHub #21) the supports object is an
//!        assembly of two parts, the body and "<name> interface" (the walls' tops and the
//!        tines), so a toolchanger can give the interface its own filament.
//!
//! We do NOT embed slicer-specific print profiles (Bambu/Orca bind "supports off" to a full
//! printer-specific project config, which breaks across printers and slicers). Bambu and Prusa
//! factory profiles default to no supports, and the fins are ordinary model geometry -- so the
//! exported file prints as intended without reaching into any slicer's settings.
//!
//! Geometry in, same as the STL path: flat arrays of [x,y,z], three vertices per triangle,
//! already in print space (oriented, seated on the plate).

use crate::json::{self, Json};
use crate::limits::{
    LimitExceeded, LoadOptions, MAX_COMPONENT_DEPTH, MAX_OBJECTS_PER_PART, MAX_TRIANGLES,
    ReadError, check_finite,
};
use crate::obj::ObjBbox;
use crate::paintcode::{FilamentPainter, Paint};
use crate::text::{decode, is_js_space, js_trim};
use crate::zip::{ZipData, ZipFile, unzip_with, zip_store};
use indexmap::{IndexMap, IndexSet};
use sf_engine::jsnum;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const NS_CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const REL_3DMODEL: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const CT_MODEL: &str = "application/vnd.ms-package.3dmanufacturing-3dmodel+xml";
const CT_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";

/// Trim a coordinate to a compact decimal string. 6 decimals is well under the ~1um that
/// matters for a print and keeps the model file small. (`fmt`, web/threemf.js:47-52.)
fn fmt(n: f64) -> String {
    if !n.is_finite() {
        return "0".to_string();
    }
    let mut s = jsnum::to_fixed(n, 6);
    if s.contains('.') {
        // s.replace(/0+$/, '').replace(/\.$/, '')
        s = s.trim_end_matches('0').to_string();
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_string() } else { s }
}

/// Port of `meshXML` (web/threemf.js:60-96): build an indexed <mesh> from a flat triangle-soup
/// array. Vertices that share exact coordinates are merged (they do, because every vertex is
/// produced by the identical transform of the same source point), which restores shared
/// topology and shrinks the file; anything that doesn't merge is left as-is and slices fine.
///
/// `tri_attrs(i)` is JS's `triAttrs` (the attribute string of input triangle `i`, or none);
/// `kept` receives the input index of every triangle written.
pub fn mesh_xml(
    tris: &[[f64; 3]],
    tri_attrs: Option<&dyn Fn(usize) -> String>,
    mut kept: Option<&mut Vec<usize>>,
) -> String {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut faces: Vec<([usize; 3], String)> = Vec::new();
    let mut t = 0;
    while t < tris.len() {
        let mut idx = [0usize; 3];
        for (i, slot) in idx.iter_mut().enumerate() {
            let p = tris[t + i];
            let key = format!(
                "{},{},{}",
                jsnum::to_js_string(p[0]),
                jsnum::to_js_string(p[1]),
                jsnum::to_js_string(p[2])
            );
            let vi = match index.get(&key) {
                Some(&vi) => vi,
                None => {
                    let vi = verts.len();
                    index.insert(key, vi);
                    verts.push(p);
                    vi
                }
            };
            *slot = vi;
        }
        // Drop any triangle that collapsed to a line/point after the merge; a
        // degenerate face is invalid 3MF and some readers reject the whole model.
        if idx[0] != idx[1] && idx[1] != idx[2] && idx[0] != idx[2] {
            let attr = tri_attrs.map_or(String::new(), |f| f(t / 3));
            faces.push((idx, attr));
            if let Some(k) = kept.as_deref_mut() {
                k.push(t / 3);
            }
        }
        t += 3;
    }

    let v: String = verts
        .iter()
        .map(|p| {
            format!(
                "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",
                fmt(p[0]),
                fmt(p[1]),
                fmt(p[2])
            )
        })
        .collect();
    let f: String = faces
        .iter()
        .map(|(t, a)| {
            format!(
                "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"{}/>",
                t[0], t[1], t[2], a
            )
        })
        .collect();
    format!("<mesh><vertices>{v}</vertices><triangles>{f}</triangles></mesh>")
}

/// `esc` (web/threemf.js:98): `String(s).replace(/[<>&"]/g, ...)`.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

/// A part's colours (GitHub #236), so a painted part keeps them through the export.
/// `PartColors = { paint, rgb }` (web/threemf.js:100-114), both sparse arrays by triangle
/// index (`None` is a hole: an uncoloured triangle):
///   paint[i]  `{ name, v }`: a slicer's painted-colour attribute, verbatim -- Bambu Studio /
///             OrcaSlicer write paint_color, PrusaSlicer its own slic3rpe:mmu_segmentation.
///             Each reads only its own name, so it goes back out under the name it came in with.
///   rgb[i]    '#RRGGBB[AA]': the core spec's colour (a <basematerials> or <m:colorgroup>
///             entry), written back as one <basematerials> table.
/// The fins stay uncoloured: they are their own object, on whatever filament the slicer gives
/// them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartColors {
    pub paint: Vec<Option<Arc<Paint>>>,
    pub rgb: Vec<Option<String>>,
}

/// `PAINT_ATTRS` (web/threemf.js:115).
pub const PAINT_ATTRS: [&str; 2] = ["paint_color", "slic3rpe:mmu_segmentation"];
const NS_SLIC3RPE: &str = "http://schemas.slic3r.org/3mf/2017/06";
/// Past the objects' ids 1-4 (one id space per file).
const MATERIALS_ID: i64 = 5;

/// An element's input to `mergeColors`: its `colors` and its `positions.length`.
pub struct MergeItem<'a> {
    pub colors: Option<&'a PartColors>,
    pub positions_len: usize,
}

fn set_at<T: Clone>(v: &mut Vec<Option<T>>, i: usize, x: T) {
    if v.len() <= i {
        v.resize(i + 1, None);
    }
    v[i] = Some(x);
}

/// Port of `mergeColors` (web/threemf.js:119-130): several objects' PartColors as one, in the
/// order their triangles are merged; `None` when none of them has any.
pub fn merge_colors(objs: &[MergeItem]) -> Option<PartColors> {
    if !objs.iter().any(|o| o.colors.is_some()) {
        return None;
    }
    let mut all = PartColors::default();
    // `off` is a JS number: a positions length that is not a multiple of 9 makes it fractional,
    // and `paint[1.33]` then sets a named property that is no array element
    let mut off = 0.0f64;
    for o in objs {
        if let Some(c) = o.colors
            && off.fract() == 0.0
        {
            let off = off as usize;
            for (i, p) in c.paint.iter().enumerate() {
                if let Some(p) = p {
                    set_at(&mut all.paint, off + i, p.clone());
                }
            }
            for (i, c) in c.rgb.iter().enumerate() {
                if let Some(c) = c {
                    set_at(&mut all.rgb, off + i, c.clone());
                }
            }
        }
        // off += o.positions.length / 9
        off += o.positions_len as f64 / 9.0;
    }
    Some(all)
}

/// `colorXML` result (web/threemf.js:132-148).
struct ColorXml<'a> {
    table: String,
    pid: String,
    colors: Option<&'a PartColors>,
    /// palette index by colour
    at: IndexMap<String, usize>,
}

impl ColorXml<'_> {
    /// `triAttrs(i)`; only meaningful when `has_tri_attrs()`.
    fn tri_attrs(&self, i: usize) -> String {
        let colors = self.colors.unwrap();
        let c = colors
            .rgb
            .get(i)
            .and_then(|c| c.as_ref())
            .filter(|c| !c.is_empty());
        let p = colors.paint.get(i).and_then(|p| p.as_ref());
        let mut s = String::new();
        if let Some(c) = c {
            s += &format!(" p1=\"{}\"", self.at[c.as_str()]);
        }
        if let Some(p) = p {
            s += &format!(" {}=\"{}\"", p.name, esc(&p.v));
        }
        s
    }
    fn has_tri_attrs(&self) -> bool {
        self.colors.is_some()
    }
}

/// The part's colours as a <basematerials> table (resource `id`) + a per-triangle attribute
/// string.
fn color_xml(colors: Option<&PartColors>, id: i64) -> ColorXml<'_> {
    let Some(c) = colors else {
        return ColorXml {
            table: String::new(),
            pid: String::new(),
            colors: None,
            at: IndexMap::new(),
        };
    };
    let mut at: IndexMap<String, usize> = IndexMap::new();
    for col in c.rgb.iter().flatten().filter(|s| !s.is_empty()) {
        let n = at.len();
        at.entry(col.clone()).or_insert(n);
    }
    let table = if at.is_empty() {
        String::new()
    } else {
        let mut t = format!("<basematerials id=\"{id}\">");
        for (i, col) in at.keys().enumerate() {
            t += &format!(
                "<base name=\"Color {}\" displaycolor=\"{}\"/>",
                i + 1,
                esc(col)
            );
        }
        t + "</basematerials>"
    };
    let pid = if at.is_empty() {
        String::new()
    } else {
        format!(" pid=\"{id}\" pindex=\"0\"")
    };
    ColorXml {
        table,
        pid,
        colors,
        at,
    }
}

/// Where a SEPARATE export opens: two objects keep their 3MF positions (a single one gets
/// centred by the slicer), and print space is centred on the origin, so the pair would sit
/// half off the bed. Shift both by one transform: the footprint's centre to (90, 90) -- on any
/// bed 180 mm or bigger -- but never closer than 5 mm to the bed's 0 edge, so a footprint
/// wider than 170 mm still starts on the bed.
pub const SEPARATE_CENTRE: f64 = 90.0;

fn bed_shift(lists: &[&[[f64; 3]]]) -> [f64; 2] {
    let (mut lx, mut ly, mut hx, mut hy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for tris in lists {
        for p in tris.iter() {
            if p[0] < lx {
                lx = p[0];
            }
            if p[0] > hx {
                hx = p[0];
            }
            if p[1] < ly {
                ly = p[1];
            }
            if p[1] > hy {
                hy = p[1];
            }
        }
    }
    if !lx.is_finite() {
        return [0.0, 0.0];
    }
    [
        jsnum::max(SEPARATE_CENTRE - (lx + hx) / 2.0, 5.0 - lx),
        jsnum::max(SEPARATE_CENTRE - (ly + hy) / 2.0, 5.0 - ly),
    ]
}

/// A slicer part's kind (`PART_KIND`'s values, web/threemf.js:340-345).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartKind {
    Negative,
    Modifier,
    Blocker,
    Enforcer,
}

impl PartKind {
    /// The JS string: `negative`, `modifier`, `blocker`, `enforcer`.
    pub fn as_str(self) -> &'static str {
        match self {
            PartKind::Negative => "negative",
            PartKind::Modifier => "modifier",
            PartKind::Blocker => "blocker",
            PartKind::Enforcer => "enforcer",
        }
    }
    /// `BAMBU_SUBTYPE` (web/threemf.js:370).
    fn bambu_subtype(self) -> &'static str {
        match self {
            PartKind::Negative => "negative_part",
            PartKind::Modifier => "modifier_part",
            PartKind::Blocker => "support_blocker",
            PartKind::Enforcer => "support_enforcer",
        }
    }
    /// `PRUSA_TYPE` (web/threemf.js:371).
    fn prusa_type(self) -> &'static str {
        match self {
            PartKind::Negative => "NegativeVolume",
            PartKind::Modifier => "ParameterModifier",
            PartKind::Blocker => "SupportBlocker",
            PartKind::Enforcer => "SupportEnforcer",
        }
    }
}

/// `PART_KIND[s]` (web/threemf.js:340-345). (JS reads an object literal, so an `Object.prototype`
/// name would hit an inherited property; those are treated as absent here.)
fn part_kind(s: &str) -> Option<PartKind> {
    match s {
        "negative_part" | "NegativeVolume" => Some(PartKind::Negative),
        "modifier_part" | "ParameterModifier" => Some(PartKind::Modifier),
        "support_blocker" | "SupportBlocker" => Some(PartKind::Blocker),
        "support_enforcer" | "SupportEnforcer" => Some(PartKind::Enforcer),
        _ => None,
    }
}

/// A non-printing part's own settings, by the config they are in (web/threemf.js:358-364):
/// `bambu` model_settings.config's [key, value] pairs, `prusa3` PrusaSlicer3_project.json's
/// volume_settings object, `prusa2` Slic3r_PE_model.config's [key, value] pairs; only the ones
/// the file had.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartSettings {
    pub bambu: Option<Vec<(String, String)>>,
    pub prusa3: Option<Json>,
    pub prusa2: Option<Vec<(String, String)>>,
}

/// A slicer's part to write back (`extras` of `writeThreeMF`: a `readThreeMF` extra with `tris`
/// in print space).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtraPart {
    pub kind: PartKind,
    pub name: String,
    pub settings: PartSettings,
    pub tris: Vec<[f64; 3]>,
}

/// One model of a plate: `{ partTris, finTris, name, iface, colors, extras }`
/// (`writePlate3MF`'s `models`, web/threemf.js:410-419).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    /// the model geometry, print space
    pub part_tris: Vec<[f64; 3]>,
    /// the fins + pad, print space (may be empty)
    pub fin_tris: Vec<[f64; 3]>,
    /// written as the model Title (and the objects' names)
    pub name: String,
    /// the interface crest's triangles (prop/crest.js)
    pub iface: Vec<[f64; 3]>,
    /// the part's PartColors, by triangle of `part_tris`
    pub colors: Option<PartColors>,
    /// a slicer's parts to write back beside the part (separate form only)
    pub extras: Vec<ExtraPart>,
}

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

fn meta_xml(pairs: &[(String, String)], typ: &str) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("<metadata{typ} key=\"{}\" value=\"{}\"/>", esc(k), esc(v)))
        .collect()
}

fn pair(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// The result of `extrasXML` (web/threemf.js:373-421).
struct Slicer {
    /// PrusaSlicer 2: the part object (one mesh holding every part)
    prusa2: Option<String>,
    /// PrusaSlicer 2: this model's entry in Slic3r_PE_model.config
    config: String,
    /// the assembly object's id (Bambu / Orca / PrusaSlicer 3)
    root: Option<i64>,
    objects: Option<Vec<String>>,
    tris: Vec<[f64; 3]>,
    bambu: String,
    prusa3: Json,
}

/// A slicer's non-printing parts written back (GitHub #104), each in the same kind it came in
/// as, its settings verbatim (PLACEMENT_KEY's note), its mesh in print space beside the part's.
/// Two layouts, by where they came from:
///   - Bambu Studio, OrcaSlicer, PrusaSlicer 3: the part object is an assembly, one component
///     per part, and BOTH configs name them -- each slicer reads its own (a Bambu file opened in
///     PrusaSlicer 3 keeps its negative parts; its modifiers come without settings, which are in
///     Bambu's own words). Neither config is read for a file whose Application is the slicer's
///     own but has no printer profile beside it (PrusaSlicer 3 refuses it, Bambu and Orca leave
///     the parts solid), so the Application stays "Support Fins".
///   - PrusaSlicer 2: one mesh holding every part, each a triangle range in
///     Slic3r_PE_model.config, the layout PrusaSlicer 2 writes and reads.
///
/// The part keeps its colours; the slicer parts are uncoloured. Ids, past the model's `base`
/// (modelXML): 1 the part, 2-4 the supports, MATERIALS_ID the colours, then the slicer parts,
/// then their assembly.
fn extras_xml(
    part_tris: &[[f64; 3]],
    extras: &[ExtraPart],
    title: &str,
    col: &ColorXml,
    base: i64,
) -> Slicer {
    let name = esc(title);
    if extras.iter().any(|e| e.settings.prusa2.is_some()) {
        let mut kept: Vec<usize> = Vec::new();
        let mut all: Vec<[f64; 3]> = part_tris.to_vec();
        let mut ends: Vec<f64> = vec![part_tris.len() as f64 / 3.0];
        for e in extras {
            all.extend(e.tris.iter().copied());
            ends.push(all.len() as f64 / 3.0);
        }
        let end0 = ends[0];
        let attrs = |i: usize| {
            if (i as f64) < end0 && col.has_tri_attrs() {
                col.tri_attrs(i)
            } else {
                String::new()
            }
        };
        let mesh = mesh_xml(&all, Some(&attrs), Some(&mut kept));
        // the ranges are of the triangles WRITTEN (meshXML drops degenerate ones)
        let firsts: Vec<usize> = ends
            .iter()
            .map(|&end| {
                kept.iter()
                    .position(|&t| t as f64 >= end)
                    .unwrap_or(kept.len())
            })
            .collect();
        // [{ kind: null, name: title, settings: [] }, ...extras]
        let mut vols = String::new();
        for k in 0..=extras.len() {
            let first = if k > 0 { firsts[k - 1] as i64 } else { 0 };
            let last = firsts[k] as i64 - 1;
            if last < first {
                continue;
            }
            let (ename, vtype, settings): (&str, &str, &[(String, String)]) = if k == 0 {
                (title, "ModelPart", &[])
            } else {
                let e = &extras[k - 1];
                (
                    if e.name.is_empty() { title } else { &e.name },
                    e.kind.prusa_type(),
                    e.settings.prusa2.as_deref().unwrap_or(&[]),
                )
            };
            let mut pairs = vec![pair("name", ename), pair("volume_type", vtype)];
            pairs.extend(settings.iter().cloned());
            vols += &format!(
                "<volume firstid=\"{first}\" lastid=\"{last}\">{}</volume>",
                meta_xml(&pairs, " type=\"volume\"")
            );
        }
        return Slicer {
            tris: extras.iter().flat_map(|e| e.tris.iter().copied()).collect(),
            prusa2: Some(format!(
                "<object id=\"{}\" type=\"model\" name=\"{name}\"{}>{mesh}</object>",
                base + 1,
                col.pid
            )),
            config: format!(
                "<object id=\"{}\" instances_count=\"1\">{}{vols}</object>",
                base + 1,
                meta_xml(&[pair("name", title)], " type=\"object\"")
            ),
            root: None,
            objects: None,
            bambu: String::new(),
            prusa3: Json::Null,
        };
    }

    // past the model's part, supports (base + 1-4) and colour table (base + MATERIALS_ID)
    let ids: Vec<i64> = (0..extras.len())
        .map(|k| base + MATERIALS_ID + 1 + k as i64)
        .collect();
    let root = base + MATERIALS_ID + 1 + extras.len() as i64;
    let mut objects: Vec<String> = extras
        .iter()
        .enumerate()
        .map(|(k, e)| {
            let nm = if e.name.is_empty() {
                e.kind.as_str()
            } else {
                &e.name
            };
            format!(
                "<object id=\"{}\" type=\"model\" name=\"{}\">{}</object>",
                ids[k],
                esc(nm),
                mesh_xml(&e.tris, None, None)
            )
        })
        .collect();
    objects.push(format!(
        "<object id=\"{root}\" type=\"model\" name=\"{name}\"><components><component objectid=\"{}\"/>{}</components></object>",
        base + 1,
        ids.iter().map(|id| format!("<component objectid=\"{id}\"/>")).collect::<String>()
    ));
    let mut bambu = format!(
        "<object id=\"{root}\">{}",
        meta_xml(&[pair("name", title)], "")
    );
    bambu += &format!(
        "<part id=\"{}\" subtype=\"normal_part\">{}</part>",
        base + 1,
        meta_xml(&[pair("name", title)], "")
    );
    for (k, e) in extras.iter().enumerate() {
        let mut pairs = vec![pair(
            "name",
            if e.name.is_empty() {
                e.kind.as_str()
            } else {
                &e.name
            },
        )];
        pairs.extend(e.settings.bambu.iter().flatten().cloned());
        bambu += &format!(
            "<part id=\"{}\" subtype=\"{}\">{}</part>",
            ids[k],
            e.kind.bambu_subtype(),
            meta_xml(&pairs, "")
        );
    }
    bambu += "</object>";
    // PrusaSlicer 3 asserts on a volume without its volume_settings object
    let mut volumes = vec![Json::Obj(vec![
        ("id".into(), Json::Num((base + 1) as f64)),
        ("type".into(), Json::Str("ModelPart".into())),
        ("volume_settings".into(), Json::Obj(vec![])),
    ])];
    for (k, e) in extras.iter().enumerate() {
        volumes.push(Json::Obj(vec![
            ("id".into(), Json::Num(ids[k] as f64)),
            ("type".into(), Json::Str(e.kind.prusa_type().into())),
            (
                "volume_settings".into(),
                e.settings.prusa3.clone().unwrap_or(Json::Obj(vec![])),
            ),
        ]));
    }
    let prusa3 = Json::Obj(vec![
        ("id".into(), Json::Num(root as f64)),
        ("object_settings".into(), Json::Obj(vec![])),
        ("volumes".into(), Json::Arr(volumes)),
    ]);
    Slicer {
        tris: extras.iter().flat_map(|e| e.tris.iter().copied()).collect(),
        prusa2: None,
        config: String::new(),
        root: Some(root),
        objects: Some(objects),
        bambu,
        prusa3,
    }
}

/// The plate's bed (`{ x, y }` mm, print space's origin its centre).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bed {
    pub x: f64,
    pub y: f64,
}

/// A slicer config file `modelXML` returns (`{ name, data }`).
struct ConfigFile {
    name: &'static str,
    data: String,
}

/// The model file for a plate's `models` (web/threemf.js:158-263, `modelXML`). Each model's ids
/// are a block of their own past the one before: the first keeps the ids a one-model file
/// always had (1 the part, 2-4 the supports, MATERIALS_ID its colours, then its slicer parts),
/// so a one-model file is what it always was. Every item of a plate of several shares one bed
/// shift, so the models keep their places to each other.
fn model_xml(
    models: &[Model],
    title: &str,
    separate: bool,
    bed: Option<Bed>,
) -> (String, Vec<ConfigFile>) {
    let mut tables: Vec<String> = Vec::new();
    let mut objects: Vec<String> = Vec::new();
    let mut items: Vec<i64> = Vec::new();
    let mut footprint: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut cfg_prusa2: Vec<String> = Vec::new();
    let mut cfg_bambu: Vec<String> = Vec::new();
    let mut cfg_prusa3: Vec<Json> = Vec::new();
    let mut base: i64 = 0;
    let mut prusa2 = false;
    let mut slic3rpe = false;
    for m in models {
        let id = |k: i64| base + k;
        let name = esc(&m.name);
        let col = color_xml(m.colors.as_ref(), id(MATERIALS_ID));
        let iface = &m.iface;
        let mut fin_tris = m.fin_tris.clone();
        // a negative part cuts every part of its object: only apart from the supports
        // can it ride along without cutting the walls built in its hole
        let slicer = if separate && !m.extras.is_empty() {
            Some(extras_xml(&m.part_tris, &m.extras, &m.name, &col, base))
        } else {
            None
        };
        tables.push(col.table.clone());
        let part_attrs = |i: usize| col.tri_attrs(i);
        objects.push(match slicer.as_ref().and_then(|s| s.prusa2.clone()) {
            Some(p) => p,
            None => format!(
                "<object id=\"{}\" type=\"model\" name=\"{name}\"{}>{}</object>",
                id(1),
                col.pid,
                mesh_xml(
                    &m.part_tris,
                    col.has_tri_attrs()
                        .then_some(&part_attrs as &dyn Fn(usize) -> String),
                    None
                )
            ),
        });
        // the part with its slicer parts is an assembly of them (extrasXML)
        if let Some(objs) = slicer.as_ref().and_then(|s| s.objects.as_ref()) {
            objects.extend(objs.iter().cloned());
        }
        if let Some(s) = slicer.as_ref().filter(|s| s.prusa2.is_some()) {
            prusa2 = true;
            cfg_prusa2.push(s.config.clone());
        } else if let Some(s) = slicer.as_ref() {
            cfg_bambu.push(s.bambu.clone());
            cfg_prusa3.push(s.prusa3.clone());
        }
        if m.colors.as_ref().is_some_and(|c| {
            c.paint
                .iter()
                .flatten()
                .any(|p| p.name.starts_with("slic3rpe:"))
        }) {
            slic3rpe = true;
        }
        let part_id = slicer.as_ref().and_then(|s| s.root).unwrap_or(id(1));
        let mut own = vec![part_id];
        // the interface crest is its own object only in the separate form; locked, it
        // is just more of the supports
        if !iface.is_empty() && !separate {
            fin_tris.extend(iface.iter().copied());
        }

        if !fin_tris.is_empty() {
            objects.push(format!(
                "<object id=\"{}\" type=\"model\" name=\"{name} supports\">{}</object>",
                id(2),
                mesh_xml(&fin_tris, None, None)
            ));
            if separate {
                // two objects (three with the interface crest), one shared transform: in
                // register, sliced apart
                own = vec![part_id, id(2)];
                if !iface.is_empty() {
                    // The crest is a PART of the supports object, not an object of its own: a
                    // slicer drops every object to the plate or refuses it (Orca: "empty first
                    // layer"), and the crest starts in mid-air. As a second component of the
                    // supports it stands on the plate with them, and takes its own filament
                    // per part.
                    objects.push(format!(
                        "<object id=\"{}\" type=\"model\" name=\"{name} interface\">{}</object>",
                        id(3),
                        mesh_xml(iface, None, None)
                    ));
                    objects.push(format!(
                        "<object id=\"{}\" type=\"model\" name=\"{name} supports\"><components><component objectid=\"{}\"/><component objectid=\"{}\"/></components></object>",
                        id(4),
                        id(2),
                        id(3)
                    ));
                    own = vec![part_id, id(4)];
                }
            } else {
                // An assembly object so the part and fins import as one locked unit while
                // remaining two distinct meshes.
                objects.push(format!(
                    "<object id=\"{}\" type=\"model\" name=\"{name}\"><components><component objectid=\"{}\"/><component objectid=\"{}\"/></components></object>",
                    id(3),
                    id(1),
                    id(2)
                ));
                own = vec![id(3)];
            }
        }
        // the slicer's parts too: a modifier reaching past the part stays on the bed
        footprint.push(m.part_tris.clone());
        footprint.push(fin_tris.clone());
        footprint.push(slicer.as_ref().map(|s| s.tris.clone()).unwrap_or_default());
        items.extend(own);
        base = slicer
            .as_ref()
            .and_then(|s| s.root)
            .unwrap_or(id(MATERIALS_ID));
    }

    // One model in the separate form, with supports, or several in either: the items
    // keep their 3MF positions (a single object gets centred by the slicer). On a bed
    // of known size every file is placed, a lone part without supports too: it stands
    // where it was moved to (ui/move.js)
    let mut t = String::new();
    if bed.is_some()
        || models.len() > 1
        || (separate && models.iter().any(|m| !m.fin_tris.is_empty()))
    {
        // placed on a bed of known size (the site's plate, its centre the origin): each
        // where it stands on it, from the bed's corner, as a slicer's bed counts
        let [tx, ty] = match bed {
            Some(b) => [b.x / 2.0, b.y / 2.0],
            None => {
                let lists: Vec<&[[f64; 3]]> = footprint.iter().map(|v| v.as_slice()).collect();
                bed_shift(&lists)
            }
        };
        t = format!(" transform=\"1 0 0 0 1 0 0 0 1 {} {} 0\"", fmt(tx), fmt(ty));
    }
    let build: String = items
        .iter()
        .map(|i| format!("<item objectid=\"{i}\"{t}/>"))
        .collect();

    let ns = if prusa2 || slic3rpe {
        format!(" xmlns:slic3rpe=\"{NS_SLIC3RPE}\"")
    } else {
        String::new()
    };
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"{NS_CORE}\"{ns}><metadata name=\"Application\">Support Fins</metadata>{}<metadata name=\"Title\">{}</metadata><resources>{}{}</resources><build>{build}</build></model>",
        if prusa2 {
            "<metadata name=\"slic3rpe:Version3mf\">1</metadata>"
        } else {
            ""
        },
        esc(title),
        tables.concat(),
        objects.concat()
    );
    let mut configs = Vec::new();
    if !cfg_prusa2.is_empty() {
        configs.push(ConfigFile {
            name: "Metadata/Slic3r_PE_model.config",
            data: format!("{XML_HEAD}<config>{}</config>", cfg_prusa2.concat()),
        });
    }
    if !cfg_bambu.is_empty() {
        configs.push(ConfigFile {
            name: "Metadata/model_settings.config",
            data: format!("{XML_HEAD}<config>{}</config>", cfg_bambu.concat()),
        });
        configs.push(ConfigFile {
            name: "Metadata/PrusaSlicer3_project.json",
            data: Json::Obj(vec![("objects".into(), Json::Arr(cfg_prusa3))]).stringify(),
        });
    }
    (xml, configs)
}

fn content_types() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"{CT_RELS}\"/><Default Extension=\"model\" ContentType=\"{CT_MODEL}\"/><Default Extension=\"json\" ContentType=\"application/json\"/><Default Extension=\"config\" ContentType=\"application/xml\"/></Types>"
    )
}

/// The site's own state, carried beside the model so a 3MF it exported can be opened again
/// with the supports the user placed by hand (#214): the drawn walls as points, the fin mode
/// and the settings. A slicer ignores an extra part in the package; a re-save in one drops it,
/// and the file then opens as a plain part.
pub const SESSION_PART: &str = "Metadata/support_fins.json";

fn root_rels() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rel0\" Target=\"/3D/3dmodel.model\" Type=\"{REL_3DMODEL}\"/></Relationships>"
    )
}

/// The options object of `writeThreeMF` (web/threemf.js:296-297); `Default` is the JS defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeMfOpts {
    /// part and fins as two objects (see the header), default false = one locked object
    pub separate: bool,
    /// the site's state, written to SESSION_PART (any JSON value; JS `JSON.stringify`s it)
    pub session: Option<Json>,
    /// the interface crest's triangles
    pub iface: Vec<[f64; 3]>,
    /// the part's PartColors, by triangle of `part_tris`
    pub colors: Option<PartColors>,
    /// a slicer's parts to write back beside the part, in the separate form only
    pub extras: Vec<ExtraPart>,
}

/// Port of `writeThreeMF` (web/threemf.js:296-300): a .3mf package (JS returns a `Blob` of these
/// bytes). `name` defaults to `'Support Fins'` in JS: callers pass it explicitly.
pub fn write_three_mf(
    part_tris: &[[f64; 3]],
    fin_tris: &[[f64; 3]],
    name: &str,
    opts: &ThreeMfOpts,
) -> Vec<u8> {
    let model = Model {
        part_tris: part_tris.to_vec(),
        fin_tris: fin_tris.to_vec(),
        name: name.to_string(),
        iface: opts.iface.clone(),
        colors: opts.colors.clone(),
        extras: opts.extras.clone(),
    };
    write_plate_3mf(&[model], name, opts.separate, opts.session.as_ref(), None)
}

/// Port of `writePlate3MF` (web/threemf.js:314-326): several models on one plate in one 3MF,
/// each already where it stands on the plate (print space). Each keeps its own objects -- the
/// part, its supports, its slicer parts -- and every item one shared bed shift: the arrangement
/// opens with its spacing kept, its footprint centred on the bed as a one-model file's is
/// (`bed_shift`) -- or, given the `bed` the models were placed on (print space's origin its
/// centre), each where it stands on it. `title` defaults in JS to `models[0]?.name ?? 'Support
/// Fins'`: callers pass it explicitly.
pub fn write_plate_3mf(
    models: &[Model],
    title: &str,
    separate: bool,
    session: Option<&Json>,
    bed: Option<Bed>,
) -> Vec<u8> {
    let (xml, configs) = model_xml(models, title, separate, bed);
    let t = |name: &str, data: String| ZipFile {
        name: name.to_string(),
        data: ZipData::Text(data),
    };
    let mut entries = vec![
        t("[Content_Types].xml", content_types()),
        t("_rels/.rels", root_rels()),
        t("3D/3dmodel.model", xml),
    ];
    for c in configs {
        entries.push(t(c.name, c.data));
    }
    if let Some(s) = session {
        entries.push(t(SESSION_PART, s.stringify()));
    }
    zip_store(&entries)
}

// ---------------------------------------------------------------- 3MF reader
//
// Reading a 3MF back in. A user who models in Fusion, Bambu Studio or FreeCAD has a .3mf in
// hand, not an STL, and telling them to go re-export is the wrong answer -- especially since a
// 3MF is the better input: it states its units, so the 1/25-scale guess never happens on the
// way in either.
//
// Parsed with a small tag scanner rather than DOMParser, for two reasons: the test suite runs
// outside a browser (no DOM), and a scanner cannot be talked into resolving an external entity
// from a file a stranger sent us.
//
// Four things here are easy to get wrong and are what separate "works on our own export" from
// "works on a real Bambu / Orca / MakerWorld file":
//   1. UNITS. The unit attribute is authoritative and is not always millimetre (inch and
//      centimetre both turn up). Everything downstream is mm.
//   2. TRANSFORMS. Geometry lives in object space; a <build><item> and every <component> may
//      carry a matrix, and they compose. Ignoring them is the "part imports offset from the
//      plate / mirrored" bug.
//   3. THE PRODUCTION EXTENSION. Bambu Studio, OrcaSlicer and everything on
//      MakerWorld/Printables split each object into its own part inside the zip
//      (3D/Objects/object_N.model), referenced from the root 3dmodel.model by a
//      <component p:path="..."/>. A reader that only parses the root part throws on these --
//      which is most real multi-object files (see issue #14).
//   4. PER-FILE ID SCOPING. Object ids are scoped to the part file they live in, NOT global --
//      Bambu reuses id="1" in every object_N.model. Resolving a component's objectid in one
//      global id->object map (as three.js's own ThreeMFLoader does) silently assembles the
//      WRONG geometry when two parts collide on an id. So an object is keyed by (partName, id),
//      never id alone.
//
// A plate can hold several objects; this reader returns them SEPARATELY (each <build><item> is
// one entry in `objects`) so the caller can let the user pick which to fin, and also a merged
// `positions` for the simple single-object path.

/// `UNIT_MM[unit]` (web/threemf.js:466-468): 3MF unit vocabulary -> millimetres.
fn unit_mm(unit: &str) -> Option<f64> {
    match unit {
        "micron" => Some(0.001),
        "millimeter" => Some(1.0),
        "centimeter" => Some(10.0),
        "inch" => Some(25.4),
        "foot" => Some(304.8),
        "meter" => Some(1000.0),
        _ => None,
    }
}

/// An element's attributes, in the order they were scanned (a repeat replaces the value).
pub type Attrs = IndexMap<String, String>;

/// What `scanXML` calls back with.
pub enum XmlEvent<'a> {
    /// `onOpen(name, attrs, selfClosing)`
    Open {
        name: &'a str,
        attrs: &'a Attrs,
        self_closing: bool,
    },
    /// `onClose(name)`
    Close(&'a str),
}

/// Port of `scanXML` (web/threemf.js:476-526): walk XML, calling `f` for every open and close
/// tag. Attribute parsing honours quotes, so a value containing '>' (legal, and Bambu writes
/// object names verbatim) does not cut the tag short. JS has two callbacks sharing state; here
/// one callback gets both kinds of event, and an `Err` from it stops the scan (a JS callback
/// throwing).
///
/// A stray `/` inside a tag that is not followed by `>` makes JS loop forever; here it is
/// `Err("3MF: malformed XML tag")` (a recorded divergence).
pub fn scan_xml(
    xml: &str,
    f: &mut dyn FnMut(XmlEvent) -> Result<(), String>,
) -> Result<(), String> {
    use crate::xmlscan::{RawEvent, scan_raw};
    scan_raw(xml, &mut |ev| match ev {
        RawEvent::Open {
            name,
            attrs,
            self_closing,
        } => f(XmlEvent::Open {
            name,
            attrs: &attrs.to_map(),
            self_closing,
        }),
        RawEvent::Close(name) => f(XmlEvent::Close(name)),
    })
    .map(|_| ())
}

/// `local` (web/threemf.js:530-533): strip any namespace prefix: a writer may emit <m:object>,
/// and the core elements are unambiguous by local name.
fn local(tag: &str) -> &str {
    match tag.find(':') {
        Some(c) => &tag[c + 1..],
        None => tag,
    }
}

/// `attrLocal` (web/threemf.js:541-545): an attribute by LOCAL name. The production extension is
/// conventionally the `p:` prefix (p:path, p:UUID), but the prefix is the file's to choose, so
/// we match on the part after the colon rather than assume `p:`.
fn attr_local<'a>(attrs: &'a Attrs, lname: &str) -> Option<&'a str> {
    if let Some(v) = attrs.get(lname) {
        return Some(v);
    }
    attrs
        .iter()
        .find(|(k, _)| local(k) == lname)
        .map(|(_, v)| v.as_str())
}

/// `partName` (web/threemf.js:550): a p:path / relationship Target -> a package part name (zip
/// entries have no leading slash; a 3MF path is absolute from the package root).
fn part_name(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

type Matrix = [f64; 12];

/// `parseMatrix` (web/threemf.js:557-562): a 3MF matrix is 12 numbers, row-major, translation
/// last, applied to a ROW vector: p' = p * M3 + t. Kept as that same flat 12 for composition.
fn parse_matrix(s: Option<&str>) -> Option<Matrix> {
    let s = s.filter(|s| !s.is_empty())?;
    let toks: Vec<f64> = js_trim(s)
        .split(is_js_space)
        .filter(|t| !t.is_empty())
        .map(jsnum::string_to_number)
        .collect();
    if toks.len() != 12 || toks.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let mut m = [0.0; 12];
    m.copy_from_slice(&toks);
    Some(m)
}

/// `compose` (web/threemf.js:565-577): b applied after a, as one matrix (row-vector convention).
fn compose(a: Option<Matrix>, b: Option<Matrix>) -> Option<Matrix> {
    let Some(a) = a else { return b };
    let Some(b) = b else { return Some(a) };
    let mut m = [0.0; 12];
    for r in 0..3 {
        for c in 0..3 {
            m[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
        }
    }
    for c in 0..3 {
        m[9 + c] = a[9] * b[c] + a[10] * b[3 + c] + a[11] * b[6 + c] + b[9 + c];
    }
    Some(m)
}

/// The determinant expression used by `parseModelXML` and `emitObject`.
fn det(m: &Matrix) -> f64 {
    m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6])
}

/// `SKIP_TYPES` (web/threemf.js:585): object types that are not printable solid geometry.
/// `support` and `surface` are not closed bodies and `other` is explicitly non-printable, so
/// pulling them in as part geometry would poison the overhang analysis. `model` and
/// `solidsupport` are kept; a missing type means `model` per the spec.
fn is_skip_type(t: &str) -> bool {
    matches!(t, "support" | "surface" | "other")
}

/// A `<component>` (web/threemf.js:668-672).
#[derive(Debug, Clone)]
struct Component {
    objectid: String,
    path: Option<String>,
    transform: Option<Matrix>,
}

/// An `<object>` of a parsed part (web/threemf.js:588 comment, 626-633).
#[derive(Debug, Clone)]
struct ObjectDef {
    typ: String,
    name: String,
    verts: Vec<f64>,
    tris: Vec<f64>,
    components: Vec<Component>,
    pid: Option<String>,
    pindex: Option<String>,
    paint: Vec<Option<Arc<Paint>>>,
    rgb: Vec<Option<String>>,
    /// `withComponents` only: the `<object>` attributes, a triangle's attributes beyond v1-v3
    /// (sparse, by triangle index) and, once the part is parsed, the shared source (`cur.source`).
    attributes: Attrs,
    tri_attrs: Vec<Option<Attrs>>,
    source: Option<Arc<SourceMesh>>,
}

/// An XML element of a model part's `<resources>` outside any `<object>` (a material table, a
/// colour group, an extension's resource): `{ name, attributes, children }`, tag name as written.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlNode {
    pub name: String,
    pub attributes: Attrs,
    pub children: Vec<XmlNode>,
}

/// The source mesh of a leaf object (`obj.source`, web/threemf.js `parseModelXML` withComponents):
/// `{ attributes, vertices, indices, triangleAttributes, properties, modelAttributes }`. Every
/// occurrence of the object shares it. Attribute values are the XML-escaped text.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceMesh {
    pub attributes: Attrs,
    pub vertices: Vec<f64>,
    /// the `v1 v2 v3` of each triangle, flat (`+a.v1`: NaN when absent)
    pub indices: Vec<f64>,
    /// a triangle's attributes beyond `v1`, `v2`, `v3`, by triangle index (`None`: it has none)
    pub triangle_attributes: Vec<Option<Attrs>>,
    pub properties: Arc<Vec<XmlNode>>,
    pub model_attributes: Option<Attrs>,
}

#[derive(Debug, Clone)]
struct Item {
    objectid: String,
    transform: Option<Matrix>,
}

/// One parsed 3D/*.model part: `{ objects, items, unit }`.
#[derive(Debug)]
struct ModelPart {
    objects: IndexMap<String, ObjectDef>,
    items: Vec<Item>,
    unit: String,
}

/// A property element as the parser holds it while the tree is open: children by index.
struct PropData {
    name: String,
    attributes: Attrs,
    children: Vec<usize>,
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `+x || 0`: the number, or 0 for NaN and zero.
fn num_or0(x: Option<&String>) -> f64 {
    let n = x.map_or(f64::NAN, |s| jsnum::string_to_number(s));
    if n == 0.0 || n.is_nan() { 0.0 } else { n }
}

/// `Number.isSafeInteger(n)`.
fn is_safe_integer(n: f64) -> bool {
    n.is_finite() && jsnum::trunc(n) == n && jsnum::abs(n) <= 9007199254740991.0
}

/// The state of a model part's parse: one XML event at a time (`event`), or the bulk of a mesh in
/// batches (`vertices`, `triangles`) when the chunked scan has already read the numbers.
struct ModelParser {
    strict: bool,
    objects: IndexMap<String, ObjectDef>,
    items: Vec<Item>,
    /// material/colour group id -> colours
    groups: IndexMap<String, Vec<String>>,
    /// paint string -> its own copy
    codes: IndexMap<String, Arc<Paint>>,
    unit: String,
    /// key of the object being filled
    cur: Option<String>,
    /// key of the <basematerials>/<colorgroup> being filled
    group: Option<String>,
    in_vertices: bool,
    /// `withComponents`: retain source topology and property resources
    with_components: bool,
    prop_nodes: Vec<PropData>,
    prop_roots: Vec<usize>,
    prop_stack: Vec<usize>,
    in_resources: bool,
    in_object: bool,
    model_attributes: Option<Attrs>,
}

impl ModelParser {
    fn new(strict: bool, with_components: bool) -> Self {
        ModelParser {
            with_components,
            prop_nodes: Vec::new(),
            prop_roots: Vec::new(),
            prop_stack: Vec::new(),
            in_resources: false,
            in_object: false,
            model_attributes: None,
            strict,
            objects: IndexMap::new(),
            items: Vec::new(),
            groups: IndexMap::new(),
            codes: IndexMap::new(),
            unit: "millimeter".to_string(),
            cur: None,
            group: None,
            in_vertices: false,
        }
    }

    fn prop_tree(&self, i: usize) -> XmlNode {
        let n = &self.prop_nodes[i];
        XmlNode {
            name: n.name.clone(),
            attributes: n.attributes.clone(),
            children: n.children.iter().map(|&c| self.prop_tree(c)).collect(),
        }
    }

    fn finish(mut self) -> ModelPart {
        if self.with_components {
            let properties: Arc<Vec<XmlNode>> =
                Arc::new(self.prop_roots.iter().map(|&r| self.prop_tree(r)).collect());
            for o in self.objects.values_mut() {
                o.source = Some(Arc::new(SourceMesh {
                    attributes: o.attributes.clone(),
                    vertices: o.verts.clone(),
                    indices: o.tris.clone(),
                    triangle_attributes: std::mem::take(&mut o.tri_attrs),
                    properties: properties.clone(),
                    model_attributes: self.model_attributes.clone(),
                }));
            }
        }
        ModelPart {
            objects: self.objects,
            items: self.items,
            unit: self.unit,
        }
    }

    fn transform(&self, s: Option<&String>) -> Result<Option<Matrix>, String> {
        let m = parse_matrix(s.map(|s| s.as_str()));
        if self.strict && s.is_some() {
            let d = m.map(|m| det(&m));
            if m.is_none() || !d.is_some_and(f64::is_finite) || jsnum::abs(d.unwrap()) < 1e-12 {
                return Err("invalid component/build transform".to_string());
            }
        }
        Ok(m)
    }

    /// A run of plain `<vertex>` elements (`x y z` already read as `+x || 0`).
    fn vertices(&mut self, xyz: &[f64]) {
        // Guard on inVertices: the beam-lattice extension has its own <v> refs,
        // and only <vertices> children are mesh points.
        if let (Some(c), true) = (&self.cur, self.in_vertices) {
            self.objects[c].verts.extend_from_slice(xyz);
        }
    }

    /// A run of `<triangle>`s that carry no attribute but v1, v2, v3 (lenient parse only).
    fn triangles(&mut self, refs: &[f64]) {
        let Some(c) = &self.cur else { return };
        let o = &mut self.objects[c];
        let first = o.tris.len() / 3;
        o.tris.extend_from_slice(refs);
        // no pid/p1 of their own: the object's pid and pindex give the colour
        if let Some(pid) = &o.pid
            && let Some(g) = self.groups.get(pid.as_str())
        {
            let n = o
                .pindex
                .as_ref()
                .map_or(0.0, |s| jsnum::string_to_number(s));
            if n >= 0.0 && jsnum::trunc(n) == n && n < g.len() as f64 {
                let col = &g[n as usize];
                if !col.is_empty() {
                    for t in first..first + refs.len() / 3 {
                        set_at(&mut o.rgb, t, col.clone());
                    }
                }
            }
        }
    }

    /// What `withComponents` keeps of an opening tag: the `<resources>` elements outside objects.
    fn track_open(&mut self, tag: &str, a: &Attrs, self_closing: bool) {
        let name = local(tag);
        if name == "resources" {
            self.in_resources = !self_closing;
        } else if name == "object" {
            self.in_object = !self_closing;
        } else if self.in_resources && !self.in_object {
            let i = self.prop_nodes.len();
            self.prop_nodes.push(PropData {
                name: tag.to_string(),
                attributes: a.clone(),
                children: Vec::new(),
            });
            match self.prop_stack.last() {
                Some(&top) => self.prop_nodes[top].children.push(i),
                None => self.prop_roots.push(i),
            }
            if !self_closing {
                self.prop_stack.push(i);
            }
        }
    }

    fn event(&mut self, ev: XmlEvent) -> Result<(), String> {
        if self.with_components
            && let XmlEvent::Open {
                name,
                attrs,
                self_closing,
            } = &ev
        {
            self.track_open(name, attrs, *self_closing);
        }
        match ev {
            XmlEvent::Open {
                name: tag,
                attrs: a,
                self_closing,
            } => match local(tag) {
                "model" => {
                    if self.with_components {
                        self.model_attributes = Some(a.clone());
                    }
                    if let Some(u) = a.get("unit").filter(|u| !u.is_empty()) {
                        self.unit = u.clone();
                    }
                }
                // Colour as the core spec states it (a material table and an index per
                // triangle). Resources precede the objects that use them, so a triangle's
                // colour resolves as it is read. Ids are per part file, like object ids.
                "basematerials" | "colorgroup" => {
                    let key = js_string(a.get("id"));
                    self.groups.insert(key.clone(), Vec::new());
                    self.group = if self_closing { None } else { Some(key) };
                }
                "base" => {
                    if let Some(g) = &self.group {
                        self.groups[g].push(a.get("displaycolor").cloned().unwrap_or_default());
                    }
                }
                "color" => {
                    if let Some(g) = &self.group {
                        self.groups[g].push(a.get("color").cloned().unwrap_or_default());
                    }
                }
                "object" => {
                    if self.objects.len() >= MAX_OBJECTS_PER_PART {
                        return Err(too_many_objects());
                    }
                    if self.strict
                        && (self.objects.contains_key(&js_string(a.get("id")))
                            || !a
                                .get("id")
                                .is_some_and(|id| is_digits(id) && !id.starts_with('0')))
                    {
                        return Err("duplicate or invalid mesh object ID".to_string());
                    }
                    let key = js_string(a.get("id"));
                    self.objects.insert(
                        key.clone(),
                        ObjectDef {
                            typ: a
                                .get("type")
                                .filter(|t| !t.is_empty())
                                .cloned()
                                .unwrap_or_else(|| "model".into()),
                            name: a.get("name").cloned().unwrap_or_default(),
                            verts: Vec::new(),
                            tris: Vec::new(),
                            components: Vec::new(),
                            pid: a.get("pid").cloned(),
                            pindex: a.get("pindex").cloned(),
                            paint: Vec::new(),
                            rgb: Vec::new(),
                            attributes: if self.with_components {
                                a.clone()
                            } else {
                                Attrs::new()
                            },
                            tri_attrs: Vec::new(),
                            source: None,
                        },
                    );
                    self.cur = if self_closing { None } else { Some(key) };
                }
                "vertices" => self.in_vertices = true,
                "vertex" => {
                    // Guard on inVertices: the beam-lattice extension has its own <v> refs,
                    // and only <vertices> children are mesh points.
                    if let (Some(c), true) = (&self.cur, self.in_vertices) {
                        if self.strict
                            && ["x", "y", "z"].iter().any(|k| {
                                a.get(*k).is_none_or(|v| {
                                    js_trim(v).is_empty() || !jsnum::string_to_number(v).is_finite()
                                })
                            })
                        {
                            return Err("invalid mesh vertex".to_string());
                        }
                        let o = &mut self.objects[c];
                        o.verts.push(num_or0(a.get("x")));
                        o.verts.push(num_or0(a.get("y")));
                        o.verts.push(num_or0(a.get("z")));
                    }
                }
                "triangle" => {
                    let Some(c) = &self.cur else { return Ok(()) };
                    if self.strict
                        && ["v1", "v2", "v3"].iter().any(|k| {
                            !a.get(*k).is_some_and(|v| is_digits(v))
                                || !is_safe_integer(jsnum::string_to_number(a.get(*k).unwrap()))
                        })
                    {
                        return Err("invalid triangle references".to_string());
                    }
                    let o = &mut self.objects[c];
                    let t = o.tris.len() / 3;
                    for k in ["v1", "v2", "v3"] {
                        o.tris
                            .push(a.get(k).map_or(f64::NAN, |v| jsnum::string_to_number(v)));
                    }
                    if self.with_components {
                        let attrs: Attrs = a
                            .iter()
                            .filter(|(k, _)| !matches!(k.as_str(), "v1" | "v2" | "v3"))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        if !attrs.is_empty() {
                            set_at(&mut o.tri_attrs, t, attrs);
                        }
                    }
                    // A slicer's painted colours (PAINT_ATTRS), kept verbatim: the string
                    // subdivides THIS triangle against its own vertex order, which the whole
                    // import -> export path keeps, so it stays valid however the part turns.
                    for name in PAINT_ATTRS {
                        let Some(v) = a.get(name).filter(|v| !v.is_empty()) else {
                            continue;
                        };
                        let own = self
                            .codes
                            .entry(v.clone())
                            .or_insert_with(|| {
                                Arc::new(Paint {
                                    name: name.to_string(),
                                    v: v.clone(),
                                })
                            })
                            .clone();
                        set_at(&mut o.paint, t, own);
                        break;
                    }
                    let pid = a.get("pid").or(o.pid.as_ref());
                    let pi = a.get("p1").or(o.pindex.as_ref());
                    if let Some(pid) = pid
                        && let Some(g) = self.groups.get(pid.as_str())
                    {
                        // groups.get(String(pid))?.[+(pi ?? 0)]
                        let n = pi.map_or(0.0, |s| jsnum::string_to_number(s));
                        if n >= 0.0 && jsnum::trunc(n) == n && n < g.len() as f64 {
                            let c = &g[n as usize];
                            if !c.is_empty() {
                                let c = c.clone();
                                set_at(&mut self.objects[self.cur.as_ref().unwrap()].rgb, t, c);
                            }
                        }
                    }
                }
                "component" => {
                    // p:path (production extension) points the objectid at a DIFFERENT part
                    // file; absent, the objectid resolves within this same file.
                    if let Some(c) = &self.cur {
                        let comp = Component {
                            objectid: js_string(a.get("objectid")),
                            path: attr_local(a, "path")
                                .filter(|p| !p.is_empty())
                                .map(String::from),
                            transform: self.transform(a.get("transform"))?,
                        };
                        self.objects[c].components.push(comp);
                    }
                }
                "item" => {
                    self.items.push(Item {
                        objectid: js_string(a.get("objectid")),
                        transform: self.transform(a.get("transform"))?,
                    });
                }
                _ => {}
            },
            XmlEvent::Close(tag) => {
                let t = local(tag);
                if self.with_components {
                    if t == "object" {
                        self.in_object = false;
                    } else if t == "resources" {
                        self.in_resources = false;
                    } else if self
                        .prop_stack
                        .last()
                        .is_some_and(|&top| self.prop_nodes[top].name == tag)
                    {
                        self.prop_stack.pop();
                    }
                }
                match t {
                    "object" => self.cur = None,
                    "vertices" => self.in_vertices = false,
                    "basematerials" | "colorgroup" => self.group = None,
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

/// Parse one 3D/*.model part into { objects, items, unit } (web/threemf.js:589-690). `strict`
/// (the CLI's single-item read) throws on malformed ids, vertices, triangles and transforms.
fn parse_model_xml(xml: &str, strict: bool, with_components: bool) -> Result<ModelPart, String> {
    let mut p = ModelParser::new(strict, with_components);
    scan_xml(xml, &mut |ev| p.event(ev))?;
    Ok(p.finish())
}

/// Model parts below this many bytes are parsed in one piece.
const PAR_XML_MIN: usize = 4 << 20;

fn too_many_objects() -> String {
    format!(
        "3MF: more than {MAX_OBJECTS_PER_PART} objects in one model part; the file is not a model"
    )
}

/// How many `<object` tags `data` holds, counted in parallel (a rough count: `<objects` and text
/// that happens to say `<object` count too). The chunked scan below holds every piece's items
/// until it is done, so a part with a hundred thousand objects is turned away before it starts.
fn count_object_tags(data: &[u8]) -> usize {
    use rayon::prelude::*;
    const CH: usize = 1 << 20;
    const TAG: &[u8] = b"<object";
    (0..data.len().div_ceil(CH))
        .into_par_iter()
        .map(|c| {
            let (s, e) = (c * CH, ((c + 1) * CH).min(data.len()));
            let win = &data[s..(e + TAG.len() - 1).min(data.len())];
            let mut n = 0;
            let mut at = 0;
            while let Some(i) = win[at..e - s].iter().position(|&b| b == b'<') {
                let i = at + i;
                if win[i..].starts_with(TAG) {
                    n += 1;
                }
                at = i + 1;
            }
            n
        })
        .sum()
}

/// `parse_model_xml` on a part's bytes: a big part is scanned in parallel pieces (`xmlscan`), and
/// everything else, or anything the pieces cannot be shown to reproduce, in one sequential scan.
/// `pieces` forces the piece count (tests).
fn parse_model_bytes(
    data: &[u8],
    strict: bool,
    pieces: Option<usize>,
    with_components: bool,
) -> Result<ModelPart, String> {
    use crate::xmlscan::{Item as Chunked, scan_chunks};
    if data.len() >= PAR_XML_MIN && count_object_tags(data) > MAX_OBJECTS_PER_PART {
        return Err(too_many_objects());
    }
    let n = pieces.unwrap_or_else(|| {
        if data.len() < PAR_XML_MIN {
            1
        } else {
            (data.len() / (2 << 20)).clamp(8, 256)
        }
    });
    if !strict
        && !with_components
        && (n > 1 || pieces.is_some())
        && let Some(chunks) = scan_chunks(data, n)
    {
        let mut p = ModelParser::new(false, false);
        for c in chunks {
            let (mut v, mut t) = (0, 0);
            for item in &c.items {
                match item {
                    Chunked::Vertices(k) => {
                        p.vertices(&c.verts[v..v + 3 * k]);
                        v += 3 * k;
                    }
                    Chunked::Triangles(k) => {
                        p.triangles(&c.tris[t..t + 3 * k]);
                        t += 3 * k;
                    }
                    Chunked::Open {
                        name,
                        attrs,
                        self_closing,
                    } => p.event(XmlEvent::Open {
                        name,
                        attrs,
                        self_closing: *self_closing,
                    })?,
                    Chunked::Close(name) => p.event(XmlEvent::Close(name))?,
                }
            }
        }
        return Ok(p.finish());
    }
    parse_model_xml(&decode(data), strict, with_components)
}

/// Whether the chunked scan accepts `data` cut into `pieces` (it gives up on anything it cannot
/// show equal to one scan); for the tests.
#[doc(hidden)]
pub fn model_scan_chunks_accepts(data: &[u8], pieces: usize) -> bool {
    crate::xmlscan::scan_chunks(data, pieces).is_some()
}

/// `parse_model_bytes` with the piece count forced, for the tests that diff it against
/// `parse_model_xml`: the two give the same objects, items and unit.
#[doc(hidden)]
pub fn model_parse_digest(
    data: &[u8],
    pieces: Option<usize>,
    chunked: bool,
) -> Result<String, String> {
    let part = if chunked {
        parse_model_bytes(data, false, pieces, false)?
    } else {
        parse_model_xml(&decode(data), false, false)?
    };
    Ok(format!(
        "{:?} {:?} {:?}",
        part.objects.iter().collect::<Vec<_>>(),
        part.items,
        part.unit
    ))
}

/// `String(x)` of an attribute that may be absent.
fn js_string(x: Option<&String>) -> String {
    x.cloned().unwrap_or_else(|| "undefined".to_string())
}

// A slicer's non-printing parts (GitHub #104). Bambu Studio, OrcaSlicer and PrusaSlicer let a
// user add a negative part (subtracted from the object), a modifier (settings for the region it
// covers) or a support blocker/enforcer -- and every one of them is an ordinary type="model"
// mesh in the 3D model. What makes it one is a slicer config beside it:
//   Bambu/Orca  Metadata/model_settings.config, <object id=ROOT><part id=N
//               subtype="negative_part">, N being the objectid of one of the root object's
//               <component>s (each part is a component);
//   PrusaSlicer 3 Metadata/PrusaSlicer3_project.json, the same shape as Bambu's as JSON:
//               objects[{ id: ROOT, volumes[{ id: N, type: "NegativeVolume" }] }];
//   PrusaSlicer 2 Metadata/Slic3r_PE_model.config, <object id=ROOT><volume firstid lastid> +
//               metadata volume_type="NegativeVolume", a range of the root object's OWN
//               triangles (one mesh holds every part).
// Read as part geometry, a negative part comes in as solid -- the hole it cut is filled, the
// fins are analysed against it, and the export prints it. So these are kept apart from the
// part, by kind. A missing or unknown type is a part.

/// `stripExt` (web/threemf.js:700): `s.replace(/\.(stl|3mf|obj|step|stp)$/i, '')`.
fn strip_ext(s: &str) -> String {
    for ext in [".stl", ".3mf", ".obj", ".step", ".stp"] {
        if s.len() >= ext.len()
            && s.is_char_boundary(s.len() - ext.len())
            && s[s.len() - ext.len()..].eq_ignore_ascii_case(ext)
        {
            return s[..s.len() - ext.len()].to_string();
        }
    }
    s.to_string()
}

/// `PLACEMENT_KEY` (web/threemf.js:711): `/^(name|matrix|volume_type|modifier|source_\w+)$/`. A
/// non-printing part's own settings ride along with it, verbatim, so the export can hand it
/// back to the slicer it came from (a modifier IS its settings: the infill or walls it sets for
/// its region). Where the part sits and what it is called are not settings: the export writes
/// those afresh, the part in print space.
fn is_placement_key(k: &str) -> bool {
    matches!(k, "name" | "matrix" | "volume_type" | "modifier")
        || k.strip_prefix("source_").is_some_and(|r| {
            !r.is_empty() && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

/// Port of `bambuComponentSettings` (web/threemf.js): raw Bambu part attributes/settings,
/// including normal parts, by object id. Keeps duplicate records and XML-escaped values: an
/// exporter must resolve ambiguity explicitly, not silently choose a material or discard
/// metadata. Used only withComponents.
fn bambu_component_settings(xml: Option<&str>) -> IndexMap<String, BambuObjectSettings> {
    let mut objects: IndexMap<String, BambuObjectSettings> = IndexMap::new();
    let Some(xml) = xml.filter(|x| !x.is_empty()) else {
        return objects;
    };
    // the object being filled (its id; `None` when the id was absent, which nothing can look up)
    let mut object: Option<Option<String>> = None;
    let mut part: Option<usize> = None;
    let mut anon = BambuObjectSettings::default();
    let _ = scan_xml(xml, &mut |ev| {
        match ev {
            XmlEvent::Open {
                name,
                attrs,
                self_closing,
            } => match local(name) {
                "object" => {
                    let id = attrs.get("id").cloned();
                    match &id {
                        Some(i) => {
                            objects.insert(i.clone(), BambuObjectSettings::default());
                        }
                        None => anon = BambuObjectSettings::default(),
                    }
                    object = if self_closing { None } else { Some(id) };
                    part = None;
                }
                "part" if object.is_some() => {
                    let o = match object.as_ref().unwrap() {
                        Some(i) => &mut objects[i.as_str()],
                        None => &mut anon,
                    };
                    o.parts.push(BambuPartSettings {
                        attributes: attrs.clone(),
                        metadata: Vec::new(),
                    });
                    part = if self_closing {
                        None
                    } else {
                        Some(o.parts.len() - 1)
                    };
                }
                "metadata" if object.is_some() && attrs.contains_key("key") => {
                    let o = match object.as_ref().unwrap() {
                        Some(i) => &mut objects[i.as_str()],
                        None => &mut anon,
                    };
                    let pair = (
                        attrs["key"].clone(),
                        attrs.get("value").cloned().unwrap_or_default(),
                    );
                    match part {
                        Some(p) => match o.parts.get_mut(p) {
                            Some(part) => part.metadata.push(pair),
                            None => o.metadata.push(pair),
                        },
                        None => o.metadata.push(pair),
                    }
                }
                _ => {}
            },
            XmlEvent::Close(name) => match local(name) {
                "part" => part = None,
                "object" => {
                    object = None;
                    part = None;
                }
                _ => {}
            },
        }
        Ok(())
    });
    objects
}

/// An object of `bambu_component_settings`: its own `<metadata>` and its `<part>`s.
#[derive(Debug, Default)]
struct BambuObjectSettings {
    metadata: Vec<(String, String)>,
    parts: Vec<BambuPartSettings>,
}

/// A root object's filament (paintcode.js), from whichever slicer config says (web/threemf.js
/// 713-722): `parts` by component objectid (Bambu/Orca, PrusaSlicer 3), `ranges` by triangle of
/// the root's own mesh (PrusaSlicer 2). 0 or absent is "the object's own".
#[derive(Debug, Default, Clone)]
struct Filament {
    object: f64,
    parts: IndexMap<String, f64>,
    ranges: Vec<FilRange>,
}

#[derive(Debug, Clone, Copy)]
struct FilRange {
    first: f64,
    last: f64,
    n: f64,
}

/// `Map` keyed by an object id, where JS also lets a `null` id be a key.
type Filaments = IndexMap<Option<String>, Filament>;

/// `filamentOf` (web/threemf.js:723-726).
fn filament_of<'a>(filaments: &'a mut Filaments, id: &Option<String>) -> &'a mut Filament {
    filaments.entry(id.clone()).or_default()
}

/// `filamentNo` (web/threemf.js:727): `/^\d+$/.test(String(v ?? '')) ? +v : 0`.
fn filament_no(v: Option<&str>) -> f64 {
    match v {
        Some(s) if is_digits(s) => jsnum::string_to_number(s),
        _ => 0.0,
    }
}

/// A non-printing part named by a slicer config (`{ kind, name, settings }`).
#[derive(Debug, Clone)]
struct RoleProto {
    kind: PartKind,
    name: String,
    settings: PartSettings,
}

type RootParts = IndexMap<String, IndexMap<String, RoleProto>>;

fn truthy(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !s.is_empty())
}

/// Port of `parseBambuConfig` (web/threemf.js:729-784): Bambu/Orca's
/// Metadata/model_settings.config: the human object names, keyed by the ROOT object id (the
/// same id the build item references), each root object's non-printing parts, keyed by the part
/// id (= the component objectid), and the filament of each object and part, into `filaments`.
/// Best-effort: absent config or unknown id just falls back to a generic label / a printing
/// part at the call site.
///
/// A config whose nesting is inconsistent (an `<object>` opened inside a `<part>` and a `<metadata>`
/// read for a part that is no longer there) is an error, not a panic (audit S-02).
fn parse_bambu_config(
    config_xml: Option<&str>,
    filaments: &mut Filaments,
) -> Result<(IndexMap<String, String>, RootParts), String> {
    let mut names: IndexMap<String, String> = IndexMap::new();
    let mut parts: RootParts = IndexMap::new();
    let Some(xml) = config_xml.filter(|x| !x.is_empty()) else {
        return Ok((names, parts));
    };
    let mut cur_id: Option<String> = None;
    let mut cur_part: Option<RoleProto> = None;
    let mut cur_part_key: Option<String> = None; // where `curPart` is stored in `parts`
    let mut cur_part_root: Option<String> = None; // ... under which root object
    let mut bad = false;
    let mut cur_part_id: Option<String> = None;
    let _ = scan_xml(xml, &mut |ev| {
        match ev {
            XmlEvent::Open {
                name: tag,
                attrs: a,
                self_closing,
            } => {
                let t = local(tag);
                // a part's filament is also one of its settings (below): recorded, not taken
                if t == "metadata"
                    && truthy(&cur_id)
                    && a.get("key").map(String::as_str) == Some("extruder")
                {
                    let n = filament_no(a.get("value").map(String::as_str));
                    let f = filament_of(filaments, &cur_id);
                    match &cur_part_id {
                        Some(pid) if !pid.is_empty() => {
                            f.parts.insert(pid.clone(), n);
                        }
                        _ => f.object = n,
                    }
                }
                if t == "object" {
                    cur_id = a.get("id").cloned();
                } else if t == "part" && truthy(&cur_id) {
                    cur_part_id = if self_closing || a.get("id").is_none() {
                        None
                    } else {
                        Some(js_string(a.get("id")))
                    };
                    let kind = a.get("subtype").and_then(|s| part_kind(s));
                    cur_part = kind.map(|kind| RoleProto {
                        kind,
                        name: String::new(),
                        settings: PartSettings {
                            bambu: Some(Vec::new()),
                            ..Default::default()
                        },
                    });
                    cur_part_key = None;
                    if let Some(p) = &cur_part {
                        let root = cur_id.clone().unwrap();
                        let key = js_string(a.get("id"));
                        parts
                            .entry(root.clone())
                            .or_default()
                            .insert(key.clone(), p.clone());
                        cur_part_key = Some(key);
                        cur_part_root = Some(root);
                    }
                    if self_closing {
                        cur_part = None;
                        cur_part_key = None;
                        cur_part_root = None;
                    }
                } else if t == "metadata"
                    && truthy(&cur_id)
                    && a.get("key").map(String::as_str) == Some("name")
                    && a.get("value").is_some_and(|v| !v.is_empty())
                {
                    let value = a.get("value").unwrap();
                    if let (Some(_), Some(key), Some(root)) =
                        (&cur_part, &cur_part_key, &cur_part_root)
                    {
                        let Some(p) = parts.get_mut(root).and_then(|m| m.get_mut(key)) else {
                            bad = true;
                            return Err(String::new());
                        };
                        if p.name.is_empty() {
                            p.name = strip_ext(value);
                        }
                        cur_part.as_mut().unwrap().name = p.name.clone();
                    } else if !names.contains_key(cur_id.as_ref().unwrap()) {
                        // The object-level <metadata key="name"> comes before any <part>'s, and
                        // we keep only the first, so a part filename never shadows the object name.
                        names.insert(cur_id.clone().unwrap(), strip_ext(value));
                    }
                } else if t == "metadata"
                    && cur_part.is_some()
                    && a.get("key")
                        .is_some_and(|k| !k.is_empty() && !is_placement_key(k))
                {
                    let pair = (
                        a.get("key").unwrap().clone(),
                        a.get("value").cloned().unwrap_or_default(),
                    );
                    let (Some(key), Some(root)) = (&cur_part_key, &cur_part_root) else {
                        bad = true;
                        return Err(String::new());
                    };
                    let Some(bambu) = parts
                        .get_mut(root)
                        .and_then(|m| m.get_mut(key))
                        .and_then(|p| p.settings.bambu.as_mut())
                    else {
                        bad = true;
                        return Err(String::new());
                    };
                    bambu.push(pair);
                }
            }
            XmlEvent::Close(tag) => match local(tag) {
                "object" => cur_id = None,
                "part" => {
                    cur_part = None;
                    cur_part_key = None;
                    cur_part_root = None;
                    cur_part_id = None;
                }
                _ => {}
            },
        }
        Ok(())
    });
    if bad {
        return Err(
            "3MF: Metadata/model_settings.config is inconsistent (a part's settings \
                    outside the part)"
                .to_string(),
        );
    }
    Ok((names, parts))
}

/// Port of `addPrusa3Parts` (web/threemf.js:792-818): PrusaSlicer 3's
/// Metadata/PrusaSlicer3_project.json: objects[{ id, volumes[{ id, type }] }], the same shape
/// as Bambu's -- the object is the build item's, each volume one of its components. Merged into
/// `parts` (parseBambuConfig's). The name is not in the JSON; emitObject takes it from the
/// volume's own object.
fn add_prusa3_parts(json_text: Option<&str>, parts: &mut RootParts, filaments: &mut Filaments) {
    let Some(text) = json_text.filter(|t| !t.is_empty()) else {
        return;
    };
    let Some(d) = json::parse(text) else { return };
    let objects: &[Json] = match d.get("objects") {
        Some(Json::Arr(a)) => a,
        _ => &[],
    };
    let present = |v: Option<&Json>| v.filter(|v| !matches!(v, Json::Null)).is_some();
    for o in objects {
        if !present(o.get("id")) {
            continue;
        }
        let oid = o.get("id").unwrap().to_js_string();
        let f = filament_of(filaments, &Some(oid.clone()));
        f.object = filament_no(
            o.get("object_settings")
                .and_then(|s| s.get("extruder"))
                .filter(|v| !matches!(v, Json::Null))
                .map(|v| v.to_js_string())
                .as_deref(),
        );
        let volumes: &[Json] = match o.get("volumes") {
            Some(Json::Arr(a)) => a,
            _ => &[],
        };
        for v in volumes {
            if present(v.get("id")) {
                let n = filament_no(
                    v.get("volume_settings")
                        .and_then(|s| s.get("extruder"))
                        .filter(|x| !matches!(x, Json::Null))
                        .map(|x| x.to_js_string())
                        .as_deref(),
                );
                filament_of(filaments, &Some(oid.clone()))
                    .parts
                    .insert(v.get("id").unwrap().to_js_string(), n);
            }
            let kind = v.get("type").and_then(|t| part_kind(&t.to_js_string()));
            let Some(kind) = kind else { continue };
            if !present(v.get("id")) {
                continue;
            }
            let vid = v.get("id").unwrap().to_js_string();
            let settings = match v.get("volume_settings") {
                Some(s @ Json::Obj(_)) => s.clone(),
                _ => Json::Obj(Vec::new()),
            };
            // our own export names a part in both configs: one part, both settings
            let entry = parts.entry(oid.clone()).or_default();
            match entry.get_mut(&vid) {
                Some(had) => had.settings.prusa3 = Some(settings),
                None => {
                    entry.insert(
                        vid,
                        RoleProto {
                            kind,
                            name: String::new(),
                            settings: PartSettings {
                                prusa3: Some(settings),
                                ..Default::default()
                            },
                        },
                    );
                }
            }
        }
    }
}

/// One non-printing volume of a PrusaSlicer 2 object: a triangle range of its mesh.
#[derive(Debug, Clone)]
struct PrusaVol {
    first: f64,
    last: f64,
    kind: PartKind,
    name: String,
    settings: Vec<(String, String)>,
}

/// Port of `parsePrusaConfig` (web/threemf.js:826-875): PrusaSlicer's
/// Metadata/Slic3r_PE_model.config: each root object's non-printing volumes as triangle ranges
/// (firstid..lastid, inclusive) of its own mesh. Older files mark a modifier with
/// key="modifier" value="1" and no volume_type.
fn parse_prusa_config(
    config_xml: Option<&str>,
    filaments: &mut Filaments,
) -> IndexMap<String, Vec<PrusaVol>> {
    struct Raw {
        first: f64,
        last: f64,
        kind: Option<PartKind>,
        name: String,
        legacy: Option<PartKind>,
        settings: Vec<(String, String)>,
    }
    let mut vols: IndexMap<String, Vec<Raw>> = IndexMap::new();
    let Some(xml) = config_xml.filter(|x| !x.is_empty()) else {
        return IndexMap::new();
    };
    let mut cur_id: Option<String> = None;
    let mut cur: Option<usize> = None; // index into vols[cur_vol_id]
    let mut cur_vol_id: Option<String> = None;
    let _ = scan_xml(xml, &mut |ev| {
        match ev {
            XmlEvent::Open {
                name: tag,
                attrs: a,
                ..
            } => {
                let t = local(tag);
                let key = a.get("key").map(String::as_str);
                if t == "object" {
                    cur_id = a.get("id").cloned();
                } else if t == "volume" && truthy(&cur_id) {
                    let c = Raw {
                        first: a
                            .get("firstid")
                            .map_or(f64::NAN, |v| jsnum::string_to_number(v)),
                        last: a
                            .get("lastid")
                            .map_or(f64::NAN, |v| jsnum::string_to_number(v)),
                        kind: None,
                        name: String::new(),
                        legacy: None,
                        settings: Vec::new(),
                    };
                    let id = cur_id.clone().unwrap();
                    let list = vols.entry(id.clone()).or_default();
                    list.push(c);
                    cur = Some(list.len() - 1);
                    cur_vol_id = Some(id);
                } else if t == "metadata"
                    && truthy(&cur_id)
                    && cur.is_none()
                    && key == Some("extruder")
                {
                    filament_of(filaments, &cur_id).object =
                        filament_no(a.get("value").map(String::as_str));
                } else if t == "metadata" && cur.is_some() {
                    let c = &mut vols[cur_vol_id.as_ref().unwrap()][cur.unwrap()];
                    if key == Some("extruder") {
                        let n = filament_no(a.get("value").map(String::as_str));
                        if n != 0.0 && c.first.is_finite() && c.last.is_finite() {
                            filament_of(filaments, &cur_id).ranges.push(FilRange {
                                first: c.first,
                                last: c.last,
                                n,
                            });
                        }
                    }
                    if key == Some("volume_type") {
                        c.kind = a.get("value").and_then(|v| part_kind(v));
                    } else if key == Some("modifier")
                        && a.get("value").map(String::as_str) == Some("1")
                    {
                        c.legacy = Some(PartKind::Modifier);
                    } else if key == Some("name") && a.get("value").is_some_and(|v| !v.is_empty()) {
                        c.name = strip_ext(a.get("value").unwrap());
                    } else if key.is_some_and(|k| !k.is_empty() && !is_placement_key(k)) {
                        c.settings.push((
                            key.unwrap().to_string(),
                            a.get("value").cloned().unwrap_or_default(),
                        ));
                    }
                }
            }
            XmlEvent::Close(tag) => match local(tag) {
                "object" => cur_id = None,
                "volume" => cur = None,
                _ => {}
            },
        }
        Ok(())
    });
    let mut out: IndexMap<String, Vec<PrusaVol>> = IndexMap::new();
    for (id, list) in vols {
        let kept: Vec<PrusaVol> = list
            .into_iter()
            .filter_map(|v| {
                let kind = v.kind.or(v.legacy)?;
                (v.first.is_finite() && v.last.is_finite()).then_some(PrusaVol {
                    first: v.first,
                    last: v.last,
                    kind,
                    name: v.name,
                    settings: v.settings,
                })
            })
            .collect();
        if !kept.is_empty() {
            out.insert(id, kept);
        }
    }
    out
}

/// A role as `emitObject` passes it down: one non-printing part, its own soup.
#[derive(Debug, Clone)]
struct Role {
    kind: PartKind,
    name: String,
    settings: PartSettings,
}

/// What a soup is keyed by in `extra`: a role by identity (`named` makes one per use) or one of
/// a PrusaSlicer 2 object's volumes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RoleKey {
    Arena(usize),
    Vol(String, usize),
}

/// A soup `sink(role)` returns: `out` the positions, `col` the part's colours (the main soup).
struct Soup {
    out: Vec<f64>,
    col: Option<PartColors>,
}

/// The roles that `emitObject` is given: `parts` (only at a BUILD ITEM's object) names its
/// components by objectid, `filament` is that item's filament entry. (The rest of JS's `roles`
/// -- `ranges`, `filRanges`, `filObject`, `paint` -- is the same at every depth and lives in
/// `Emit`.)
#[derive(Clone, Copy)]
struct Roles<'a> {
    parts: Option<&'a IndexMap<String, RoleProto>>,
    filament: Option<&'a Filament>,
}

struct Stats {
    meshes: usize,
    skipped: usize,
    dropped: usize,
    missing: usize,
    cycles: usize,
}

/// The state `emitObject` reads: parsed parts (lazily, cached), the slicer configs and the
/// per-item sinks.
struct Emit<'a> {
    parts: &'a IndexMap<String, Vec<u8>>,
    parsed: IndexMap<String, Option<Rc<ModelPart>>>,
    strict: bool,
    /// `withComponents`: parts keep their source mesh and property resources
    with_components: bool,
    root_name: String,
    prusa_vols: IndexMap<String, Vec<PrusaVol>>,
    filaments: Filaments,
    painter: Option<FilamentPainter>,
    arena: Vec<Role>,
    main: Soup,
    extra: IndexMap<RoleKey, Soup>,
    /// `withComponents`: one record per leaf mesh occurrence, for the build item being read
    components: Option<Vec<CompRec>>,
    /// triangles emitted so far, over every build item (the running guard of the 20M limit)
    emitted: u64,
    allow_large: bool,
}

/// One leaf mesh occurrence while `emitObject` runs (`record` in web/threemf.js `emitObject`);
/// `target` is the soup its triangles went to (`None`: the main one).
struct CompRec {
    path: String,
    object_id: String,
    part_id: Option<String>,
    route: Vec<usize>,
    kind: &'static str,
    transform: Option<Matrix>,
    corner_order: [u8; 3],
    source: Option<Arc<SourceMesh>>,
    source_faces: Vec<u32>,
    first_triangle: usize,
    target: Option<RoleKey>,
}

impl Emit<'_> {
    /// `getPart(name)`: parse each part on demand and cache it -- a plate can reference the same
    /// object_N.model many times, and most parts in a big package go untouched.
    fn get_part(&mut self, name: &str) -> Result<Option<Rc<ModelPart>>, String> {
        if let Some(p) = self.parsed.get(name) {
            return Ok(p.clone());
        }
        let p = match self.parts.get(name) {
            Some(data) => Some(Rc::new(parse_model_bytes(
                data,
                self.strict,
                None,
                self.with_components,
            )?)),
            None => None,
        };
        self.parsed.insert(name.to_string(), p.clone());
        Ok(p)
    }

    fn sink(&mut self, key: Option<RoleKey>) -> &mut Soup {
        match key {
            None => &mut self.main,
            Some(k) => self.extra.entry(k).or_insert_with(|| Soup {
                out: Vec::new(),
                col: None,
            }),
        }
    }
}

/// `named` (web/threemf.js:906): a config's role, copied per use: each occurrence gets its own
/// soup (and name).
fn named(arena: &mut Vec<Role>, r: Option<&RoleProto>) -> Option<usize> {
    r.map(|r| {
        arena.push(Role {
            kind: r.kind,
            name: r.name.clone(),
            settings: r.settings.clone(),
        });
        arena.len() - 1
    })
}

fn vert(verts: &[f64], o: f64) -> f64 {
    // verts[o]: a fractional or out-of-range index reads `undefined` (NaN once used)
    if o.is_finite() && jsnum::trunc(o) == o && o >= 0.0 && (o as usize) < verts.len() {
        verts[o as usize]
    } else {
        f64::NAN
    }
}

/// The refusal of a component chain deeper than [`MAX_COMPONENT_DEPTH`] (audit X-02).
fn depth_error() -> ReadError {
    format!("3MF: components are nested more than {MAX_COMPONENT_DEPTH} levels deep; the file is not a model")
        .into()
}

/// How many triangles `emit_object` would emit for object `id` of `path` (every triangle of every
/// occurrence, a DAG re-expanded at each reference as the emit does), without emitting any. A
/// memo makes this linear in the number of objects however large the answer is (saturating).
/// Skip-type objects count 0; a cycle edge counts 0, as the emit's `seen` makes it.
fn count_tris(
    e: &mut Emit,
    path: &str,
    id: &str,
    memo: &mut HashMap<(String, String), u64>,
    visiting: &mut IndexSet<(String, String)>,
) -> Result<u64, ReadError> {
    let key = (path.to_string(), id.to_string());
    if let Some(&n) = memo.get(&key) {
        return Ok(n);
    }
    if visiting.contains(&key) {
        return Ok(0);
    }
    if visiting.len() >= MAX_COMPONENT_DEPTH {
        return Err(depth_error());
    }
    let part = e.get_part(path)?;
    let Some(obj) = part.as_ref().and_then(|p| p.objects.get(id)) else {
        return Ok(0);
    };
    if is_skip_type(&obj.typ) {
        return Ok(0);
    }
    visiting.insert(key.clone());
    let mut n = (obj.tris.len() / 3) as u64;
    for c in &obj.components {
        let child_path = c.path.as_deref().map_or(path, part_name).to_string();
        n = n.saturating_add(count_tris(e, &child_path, &c.objectid, memo, visiting)?);
    }
    visiting.shift_remove(&key);
    memo.insert(key, n);
    Ok(n)
}

/// Port of `emitObject` (web/threemf.js:877-967): flatten one object (mesh or assembly) into the
/// sinks as a triangle soup, applying `m`. `path` is the part file the id lives in. A component
/// may point at a DIFFERENT part via p:path, so id lookup and cycle detection are both keyed by
/// (path,id), never id alone -- two Bambu parts legally reuse id="1".
///
/// `seen` breaks a component cycle: a malformed file can reference itself, the spec forbids it,
/// so bailing is correct rather than recursing forever.
///
/// `role` is the one this subtree inherits (None is the part). `roles` is what the slicer configs
/// say: `parts`, only at a BUILD ITEM's object, names its components by objectid (Bambu/Orca,
/// PrusaSlicer 3); the PrusaSlicer 2 ranges name a mesh's triangles by index at any depth (a
/// second instance is a wrapper object pointing at the first one's mesh, and the config names
/// only the mesh's object). A part used twice is two extras, each its own shell, as the slicer
/// has them.
///
/// `fil` is the filament this subtree is on (`Filament`: the build item's object, then a
/// component's part, then a PrusaSlicer 2 range of the mesh), and the painter gives a triangle's
/// paint for it (paintcode.js `filamentPainter`).
#[allow(clippy::too_many_arguments)]
fn emit_object(
    e: &mut Emit,
    path: &str,
    id: &str,
    m: Option<Matrix>,
    seen: &mut IndexSet<String>,
    stats: &mut Stats,
    role: Option<usize>,
    roles: Roles,
    orient: bool,
    mut fil: f64,
    route: &[usize],
    part_id: Option<&str>,
) -> Result<(), ReadError> {
    let part = e.get_part(path)?;
    let Some(obj) = part.as_ref().and_then(|p| p.objects.get(id)) else {
        stats.missing += 1; // dangling ref: a broken file
        return Ok(());
    };
    let key = format!("{path} {id}");
    if seen.contains(&key) {
        stats.cycles += 1;
        return Ok(());
    }
    // Bambu writes a non-printing part as type="other": the config naming it wins,
    // so it is kept by kind instead of skipped as unknown geometry.
    if is_skip_type(&obj.typ) && role.is_none() {
        stats.skipped += 1;
        return Ok(());
    }
    if let Some(r) = role
        && e.arena[r].name.is_empty()
        && !obj.name.is_empty()
    {
        e.arena[r].name = strip_ext(&obj.name);
    }
    if seen.len() >= MAX_COMPONENT_DEPTH {
        return Err(depth_error());
    }
    seen.insert(key.clone());

    let (verts, tris) = (&obj.verts, &obj.tris);
    e.emitted = e.emitted.saturating_add((tris.len() / 3) as u64);
    if !e.allow_large && e.emitted > MAX_TRIANGLES {
        return Err(LimitExceeded::triangles(e.emitted).into());
    }
    let at_root = path == e.root_name;
    let ranges: Option<Vec<PrusaVol>> = if role.is_some() || !at_root {
        None
    } else {
        e.prusa_vols.get(id).cloned()
    };
    let fil_ranges: Option<Vec<FilRange>> = if at_root {
        e.filaments
            .get(&Some(id.to_string()))
            .map(|f| f.ranges.clone())
    } else {
        None
    };
    // PrusaSlicer 2 names the mesh's object, which a second instance only points at
    if fil == 0.0 || fil.is_nan() {
        fil = if at_root {
            e.filaments
                .get(&Some(id.to_string()))
                .map_or(0.0, |f| f.object)
        } else {
            0.0
        };
    }
    let mirrored = orient && m.is_some_and(|m| det(&m) < 0.0);
    // `groups`: the record of each role this object's triangles went to, by role
    let mut groups: IndexMap<Option<RoleKey>, usize> = IndexMap::new();
    let mut record = |e: &mut Emit, r: Option<RoleKey>| -> usize {
        if let Some(&i) = groups.get(&r) {
            return i;
        }
        let kind = match &r {
            None => "model",
            Some(RoleKey::Arena(i)) => e.arena[*i].kind.as_str(),
            Some(RoleKey::Vol(id, i)) => e.prusa_vols[id.as_str()][*i].kind.as_str(),
        };
        let first_triangle = e.sink(r.clone()).out.len() / 9;
        let comps = e.components.as_mut().unwrap();
        comps.push(CompRec {
            path: path.to_string(),
            object_id: id.to_string(),
            part_id: part_id.map(String::from),
            route: route.to_vec(),
            kind,
            transform: m,
            corner_order: if mirrored { [0, 2, 1] } else { [0, 1, 2] },
            source: obj.source.clone(),
            source_faces: Vec::new(),
            first_triangle,
            target: r.clone(),
        });
        groups.insert(r, comps.len() - 1);
        comps.len() - 1
    };
    // Keep empty leaf occurrences visible to a strict project caller.
    if e.components.is_some() && tris.is_empty() && obj.components.is_empty() {
        record(e, role.map(RoleKey::Arena));
    }
    if ranges.is_none() && role.is_none() {
        // the whole mesh lands in the main soup: size it once instead of doubling it
        e.main.out.reserve(tris.len() / 3 * 9);
    }
    let mut part_tris = 0usize;
    let mut t = 0;
    while t < tris.len() {
        // A triangle indexing a vertex that does not exist is a broken file. Check
        // all three FIRST and drop the whole face -- pushing NaN would silently
        // blank the render, and pushing a partial face would shift every vertex
        // after it by one, shearing the rest of the mesh.
        let (o0, o1, o2) = (tris[t] * 3.0, tris[t + 1] * 3.0, tris[t + 2] * 3.0);
        let vl = verts.len() as f64;
        if !(o0 >= 0.0 && o1 >= 0.0 && o2 >= 0.0) || o0 + 3.0 > vl || o1 + 3.0 > vl || o2 + 3.0 > vl
        {
            stats.dropped += 1;
            t += 3;
            continue;
        }
        let tri = (t / 3) as f64;
        let r: Option<RoleKey> = match &ranges {
            Some(rs) => match rs.iter().position(|v| tri >= v.first && tri <= v.last) {
                Some(i) => Some(RoleKey::Vol(id.to_string(), i)),
                None => role.map(RoleKey::Arena),
            },
            None => role.map(RoleKey::Arena),
        };
        if e.components.is_some() {
            let i = record(e, r.clone());
            e.components.as_mut().unwrap()[i]
                .source_faces
                .push((t / 3) as u32);
        }
        let is_main = r.is_none();
        if is_main {
            part_tris += 1;
        }
        if is_main {
            // `col` is the main soup's colours
            let k = e.main.out.len() / 9;
            let n = {
                let n = fil_ranges
                    .as_ref()
                    .and_then(|rs| rs.iter().find(|v| tri >= v.first && tri <= v.last))
                    .map_or(0.0, |v| v.n);
                if n != 0.0 { n } else { fil }
            };
            let own = obj.paint.get(t / 3).and_then(|p| p.as_ref());
            let p = match &mut e.painter {
                Some(painter) => painter.paint(own, n),
                None => own.cloned(),
            };
            let col = e.main.col.as_mut().unwrap();
            if let Some(p) = p {
                set_at(&mut col.paint, k, p);
            }
            if let Some(c) = obj
                .rgb
                .get(t / 3)
                .and_then(|c| c.as_ref())
                .filter(|c| !c.is_empty())
            {
                set_at(&mut col.rgb, k, c.clone());
            }
        }
        let out = &mut e.sink(r).out;
        let order = if mirrored { [o0, o2, o1] } else { [o0, o1, o2] };
        for o in order {
            let (x, y, z) = (vert(verts, o), vert(verts, o + 1.0), vert(verts, o + 2.0));
            match m {
                Some(m) => {
                    out.push(x * m[0] + y * m[3] + z * m[6] + m[9]);
                    out.push(x * m[1] + y * m[4] + z * m[7] + m[10]);
                    out.push(x * m[2] + y * m[5] + z * m[8] + m[11]);
                }
                None => {
                    out.push(x);
                    out.push(y);
                    out.push(z);
                }
            }
        }
        t += 3;
    }
    if part_tris > 0 {
        stats.meshes += 1;
    }

    for (component_index, c) in obj.components.iter().enumerate() {
        let child_path = c.path.as_deref().map_or(path, part_name).to_string();
        let mut child_route = route.to_vec();
        if e.components.is_some() {
            child_route.push(component_index);
        }
        let child_role = match named(&mut e.arena, roles.parts.and_then(|p| p.get(&c.objectid))) {
            Some(r) => Some(r),
            None => role,
        };
        let child_fil = match roles
            .filament
            .and_then(|f| f.parts.get(&c.objectid))
            .copied()
        {
            Some(n) if n != 0.0 && !n.is_nan() => n,
            _ => fil,
        };
        emit_object(
            e,
            &child_path,
            &c.objectid,
            compose(c.transform, m),
            seen,
            stats,
            child_role,
            Roles {
                parts: None,
                filament: None,
            },
            orient,
            child_fil,
            &child_route,
            if route.is_empty() {
                Some(&c.objectid)
            } else {
                part_id
            },
        )?;
    }
    seen.shift_remove(&key);
    Ok(())
}

/// `bbox` (web/threemf.js:969-977). Min and max do not depend on the order they are taken in (a NaN
/// never replaces a bound), so a big soup is reduced in parallel with the same result.
fn bbox(positions: &[f64]) -> ObjBbox {
    use rayon::prelude::*;
    let part = |ch: &[f64]| {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in ch.as_chunks::<3>().0 {
            for k in 0..3 {
                if p[k] < lo[k] {
                    lo[k] = p[k];
                }
                if p[k] > hi[k] {
                    hi[k] = p[k];
                }
            }
        }
        (lo, hi)
    };
    let (lo, hi) = positions.par_chunks(3 * 32768).map(part).reduce(
        || ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(l1, h1), (l2, h2)| {
            (
                std::array::from_fn(|k| if l2[k] < l1[k] { l2[k] } else { l1[k] }),
                std::array::from_fn(|k| if h2[k] > h1[k] { h2[k] } else { h1[k] }),
            )
        },
    );
    ObjBbox {
        lo,
        hi,
        size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
    }
}

/// `findRootPart` (web/threemf.js:980-1004): locate the root 3dmodel.model part name (the one
/// carrying <build>).
fn find_root_part(parts: &IndexMap<String, Vec<u8>>) -> Option<String> {
    if parts.contains_key("3D/3dmodel.model") {
        return Some("3D/3dmodel.model".to_string());
    }
    // The root relationship names it; use that before guessing.
    if let Some(rels) = parts.get("_rels/.rels") {
        let mut target: Option<String> = None;
        let _ = scan_xml(&decode(rels), &mut |ev| {
            if let XmlEvent::Open { name, attrs: a, .. } = ev
                && local(name) == "Relationship"
                && a.get("Type").map(String::as_str) == Some(REL_3DMODEL)
                && let Some(t) = a.get("Target").filter(|t| !t.is_empty())
            {
                target = Some(t.clone());
            }
            Ok(())
        });
        if let Some(t) = target
            && parts.contains_key(part_name(&t))
        {
            return Some(part_name(&t).to_string());
        }
    }
    // Odd writer that renamed it: among the .model parts, the root is the one with
    // a <build> section (the object_N.model parts have none). Fall back to first.
    let mut first: Option<String> = None;
    for (name, data) in parts {
        if !name.to_lowercase().ends_with(".model") {
            continue;
        }
        if first.is_none() {
            first = Some(name.clone());
        }
        if has_build_tag(&decode(data)) {
            return Some(name.clone());
        }
    }
    first
}

/// `/<\s*build[\s>]/.test(text)`.
fn has_build_tag(text: &str) -> bool {
    let mut from = 0;
    while let Some(x) = text[from..].find('<') {
        let lt = from + x;
        let rest = text[lt + 1..].trim_start_matches(is_js_space);
        if let Some(after) = rest.strip_prefix("build")
            && after
                .chars()
                .next()
                .is_some_and(|c| is_js_space(c) || c == '>')
        {
            return true;
        }
        from = lt + 1;
    }
    false
}

/// One non-printing part `readThreeMF` returns (web/threemf.js:1052-1054 comment): `{ kind, name,
/// settings, positions }`, in the same space as the object's `positions` but NOT in it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadExtra {
    pub kind: PartKind,
    pub name: String,
    pub settings: PartSettings,
    pub positions: Vec<f64>,
}

/// A Bambu `<part>` of `model_settings.config`, raw (web/threemf.js `bambuComponentSettings`):
/// its attributes and every `<metadata key value>` in order, XML-escaped values kept.
#[derive(Debug, Clone, PartialEq)]
pub struct BambuPartSettings {
    pub attributes: Attrs,
    pub metadata: Vec<(String, String)>,
}

/// One leaf mesh occurrence of a build item (`withComponents`, web/threemf.js `readThreeMFParts`
/// doc comment): where it came from, its source mesh, and its triangles in the item's positions.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentInfo {
    /// the model part the leaf lives in
    pub path: String,
    pub object_id: String,
    /// the root component's object id this occurrence sits under (`None` at the item itself)
    pub part_id: Option<String>,
    /// the component index at each level, from the build item down
    pub route: Vec<usize>,
    /// `model`, or the kind of the non-printing part
    pub kind: &'static str,
    /// composed BEFORE `unit_scale`
    pub transform: Option<[f64; 12]>,
    /// which source corner each emitted corner is
    pub corner_order: [u8; 3],
    pub source: Option<Arc<SourceMesh>>,
    /// relative to the main mesh or that extra's mesh
    pub first_triangle: usize,
    pub build_index: usize,
    pub unit_scale: f64,
    /// the original triangle index of each emitted triangle
    pub source_faces: Vec<u32>,
    /// world-mm positions of those triangles (a copy of the item's)
    pub positions: Vec<f64>,
    pub settings: Vec<BambuPartSettings>,
    pub object_settings: Vec<(String, String)>,
}

/// One entry of `readThreeMF(..).objects`.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeMfObject {
    pub name: String,
    /// `Float32Array` (or `Float64Array` with `double_precision`): flat [x,y,z] x 3 per triangle
    pub positions: Vec<f64>,
    pub tris: usize,
    pub meshes: usize,
    pub skipped: usize,
    pub dropped: usize,
    pub bbox: ObjBbox,
    /// `null`, or the object's colours by triangle index (see PartColors)
    pub colors: Option<PartColors>,
    pub extras: Vec<ReadExtra>,
    /// `withComponents` only
    pub components: Option<Vec<ComponentInfo>>,
}

/// What `readThreeMF` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeMf {
    pub unit: String,
    pub objects: Vec<ThreeMfObject>,
    /// every object merged -- the simple single-part path
    pub positions: Vec<f64>,
    /// aggregate across objects
    pub meshes: usize,
    pub items: usize,
    pub skipped: usize,
    pub dropped: usize,
    /// SESSION_PART's object when the site wrote one, else `None`
    pub session: Option<Json>,
    /// SESSION_PART is in the file but could not be used (damaged, nested deeper than the JSON limit, or
    /// not a version-1 session): the JS ignores it silently, the app says so
    pub session_unreadable: bool,
}

/// Port of `readThreeMF` (web/threemf.js:1056-1059): read a .3mf package into per-object
/// triangle soups in millimetres. JS is `async` only for `unzip`'s inflate.
pub fn read_three_mf(bytes: &[u8]) -> Result<ThreeMf, String> {
    read_package(bytes, true, &LoadOptions::default()).map_err(String::from)
}

/// `read_three_mf` without the merged soup: `positions` is left empty (a caller that takes the
/// objects one by one has no use for the second copy of every triangle).
pub fn read_three_mf_unmerged(bytes: &[u8]) -> Result<ThreeMf, String> {
    read_package(bytes, false, &LoadOptions::default()).map_err(String::from)
}

/// [`read_three_mf_unmerged`] with the load options (`allow_large`), and the load-anyway limits
/// as a [`ReadError::Limit`] instead of a message. The limits are checked before the work they
/// guard: the ZIP's declared sizes before anything is inflated, the triangle count of the whole
/// build before anything is emitted.
pub fn read_three_mf_unmerged_with(bytes: &[u8], opts: &LoadOptions) -> Result<ThreeMf, ReadError> {
    read_package(bytes, false, opts).and_then(finite)
}

/// [`read_three_mf`] with the load options.
pub fn read_three_mf_with(bytes: &[u8], opts: &LoadOptions) -> Result<ThreeMf, ReadError> {
    read_package(bytes, true, opts).and_then(finite)
}

/// Refuse a package whose geometry has a coordinate that is not a number (audit X-07). Only the
/// hostile-input-safe entries do: the JS carries a missing vertex on as NaN, and the parity
/// corpus pins that.
fn finite(m: ThreeMf) -> Result<ThreeMf, ReadError> {
    for o in &m.objects {
        check_finite(&o.positions, "this 3MF")?;
        for x in &o.extras {
            check_finite(&x.positions, "this 3MF")?;
        }
    }
    check_finite(&m.positions, "this 3MF")?;
    Ok(m)
}

fn read_package(bytes: &[u8], merge: bool, load: &LoadOptions) -> Result<ThreeMf, ReadError> {
    let mut parts = unzip_with(bytes, load)?;
    // The root model part is by far the biggest thing in the package (633 MB of XML for a plate
    // of 7.7M triangles). Parse it now and let go of its bytes before the soups are built.
    let pre = match find_root_part(&parts) {
        Some(name) if parts.contains_key(&name) => {
            let data = std::mem::take(parts.get_mut(&name).unwrap());
            let part = parse_model_bytes(&data, false, None, false)?;
            Some((name, part))
        }
        _ => None,
    };
    read_three_mf_core(&parts, &ReadOpts::default(), load, pre, merge)
}

/// The options object of `readThreeMFParts`; `Default` is the JS defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadOpts {
    /// read only this build item (`buildIndex`; `None` is JS `null`). Also switches the reader
    /// to strict parsing.
    pub build_index: Option<usize>,
    /// keep float64 positions
    pub double_precision: bool,
    /// flip triangles under a mirrored transform so they stay outward for analysis
    pub outward_winding: bool,
    /// add one record per leaf mesh occurrence (`ThreeMfObject::components`); opt-in: ordinary
    /// callers neither retain this data nor pay for its parsing
    pub with_components: bool,
}

/// Port of `readThreeMFParts` (web/threemf.js:1068-1196): already opened package members. The CLI
/// project editor loads just the selected build item's dependencies, one item at a time; it
/// never rewrites these meshes. The normal browser import uses neither option and retains its
/// existing float32/paint order.
pub fn read_three_mf_parts(
    parts: &IndexMap<String, Vec<u8>>,
    opts: &ReadOpts,
) -> Result<ThreeMf, String> {
    read_three_mf_core(parts, opts, &LoadOptions::default(), None, true).map_err(String::from)
}

/// `read_three_mf_parts`, with the root model part optionally parsed already (`pre`: its name and
/// parse); its bytes in `parts` are then never read.
fn read_three_mf_core(
    parts: &IndexMap<String, Vec<u8>>,
    opts: &ReadOpts,
    load: &LoadOptions,
    pre: Option<(String, ModelPart)>,
    merge: bool,
) -> Result<ThreeMf, ReadError> {
    let strict = opts.build_index.is_some();
    let root_name = match (&pre, find_root_part(parts)) {
        (Some((n, _)), _) => n.clone(),
        (None, Some(n)) if parts.contains_key(&n) => n,
        _ => return Err("no 3D model part found in this 3MF".into()),
    };

    let mut e = Emit {
        parts,
        parsed: IndexMap::new(),
        strict,
        with_components: opts.with_components,
        root_name: root_name.clone(),
        prusa_vols: IndexMap::new(),
        filaments: Filaments::new(),
        painter: None,
        arena: Vec::new(),
        main: Soup {
            out: Vec::new(),
            col: None,
        },
        extra: IndexMap::new(),
        components: None,
        emitted: 0,
        allow_large: load.allow_large,
    };
    if let Some((name, part)) = pre {
        e.parsed.insert(name, Some(Rc::new(part)));
    }
    let root = e.get_part(&root_name)?.unwrap();
    let unit = root.unit.clone();
    let scale = unit_mm(&unit).unwrap_or(1.0);
    let text = |name: &str| parts.get(name).map(|d| decode(d));
    let bambu = text("Metadata/model_settings.config");
    let (names, mut slicer_parts) = parse_bambu_config(bambu.as_deref(), &mut e.filaments)?;
    let component_settings = opts
        .with_components
        .then(|| bambu_component_settings(bambu.as_deref()));
    add_prusa3_parts(
        text("Metadata/PrusaSlicer3_project.json").as_deref(),
        &mut slicer_parts,
        &mut e.filaments,
    );
    e.prusa_vols = parse_prusa_config(
        text("Metadata/Slic3r_PE_model.config").as_deref(),
        &mut e.filaments,
    );
    // a part's filament goes out as paint (paintcode.js), in the file's slicer's words
    if !e.filaments.is_empty() {
        e.painter = Some(FilamentPainter::new(
            if bambu.as_deref().is_some_and(|b| !b.is_empty()) {
                "paint_color"
            } else {
                "slic3rpe:mmu_segmentation"
            },
        ));
    }

    // Each <build><item> is one pickable object. No <build> is a valid-but-empty
    // plate; a few CAD exporters omit it, so fall back to the mesh/assembly objects
    // in the root part rather than refusing a file that plainly contains geometry.
    let roots: Vec<Item> = if !root.items.is_empty() {
        root.items.clone()
    } else {
        root.objects
            .iter()
            .filter(|(_, o)| !o.tris.is_empty() || !o.components.is_empty())
            .map(|(id, _)| Item {
                objectid: id.clone(),
                transform: None,
            })
            .collect()
    };

    let to_mm = |out: &[f64]| -> Vec<f64> {
        use rayon::prelude::*;
        out.par_iter()
            .with_min_len(1 << 15)
            .map(|&v| {
                let p = v * scale;
                if opts.double_precision {
                    p
                } else {
                    (p as f32) as f64
                }
            })
            .collect()
    };

    // The whole build's triangle count, before a triangle is emitted: a few hundred bytes of
    // components can expand to billions (audit X-03).
    if !load.allow_large {
        let mut memo = HashMap::new();
        let mut total = 0u64;
        for (index, item) in roots.iter().enumerate() {
            if opts.build_index.is_some_and(|b| index != b) {
                continue;
            }
            let n = count_tris(
                &mut e,
                &root_name,
                &item.objectid,
                &mut memo,
                &mut IndexSet::new(),
            )?;
            total = total.saturating_add(n);
        }
        if total > MAX_TRIANGLES {
            return Err(LimitExceeded::triangles(total).into());
        }
    }

    let mut objects: Vec<ThreeMfObject> = Vec::new();
    let (mut agg_meshes, mut agg_skipped, mut agg_dropped) = (0, 0, 0);
    for (index, item) in roots.iter().enumerate() {
        if opts.build_index.is_some_and(|b| index != b) {
            continue;
        }
        e.main = Soup {
            out: Vec::new(),
            col: Some(PartColors::default()),
        };
        e.extra = IndexMap::new();
        e.arena = Vec::new();
        e.components = opts.with_components.then(Vec::new);
        let mut stats = Stats {
            meshes: 0,
            skipped: 0,
            dropped: 0,
            missing: 0,
            cycles: 0,
        };
        let filament = e.filaments.get(&Some(item.objectid.clone())).cloned();
        let roles = Roles {
            parts: slicer_parts.get(&item.objectid),
            filament: filament.as_ref(),
        };
        emit_object(
            &mut e,
            &root_name,
            &item.objectid,
            item.transform,
            &mut IndexSet::new(),
            &mut stats,
            None,
            roles,
            opts.outward_winding,
            filament.as_ref().map_or(0.0, |f| f.object),
            &[],
            None,
        )?;
        if strict && (stats.missing != 0 || stats.dropped != 0 || stats.cycles != 0) {
            return Err("selected object has missing resources, cyclic components or invalid triangle references".into());
        }
        // Fold stats first: a build item that was ALL support/dangling emits nothing
        // but its skipped count still has to be reported, not dropped with the item.
        agg_meshes += stats.meshes;
        agg_skipped += stats.skipped;
        agg_dropped += stats.dropped;
        // all-skipped/dangling item, or one with only non-printing parts: not a pickable body
        if e.main.out.is_empty() {
            continue;
        }

        let positions = to_mm(&e.main.out);
        let col = e.main.col.take().unwrap();
        let root_obj = root.objects.get(&item.objectid);
        let name = names
            .get(&item.objectid)
            .filter(|n| !n.is_empty())
            .cloned()
            .or_else(|| root_obj.map(|o| o.name.clone()).filter(|n| !n.is_empty()))
            .unwrap_or_else(|| format!("Object {}", objects.len() + 1));
        let mut extra_positions: IndexMap<&RoleKey, Vec<f64>> =
            e.extra.iter().map(|(k, s)| (k, to_mm(&s.out))).collect();
        let components: Option<Vec<ComponentInfo>> = e.components.take().map(|recs| {
            let config = component_settings
                .as_ref()
                .and_then(|c| c.get(&item.objectid));
            recs.into_iter()
                .map(|c| {
                    let all: &[f64] = match &c.target {
                        None => &positions,
                        Some(k) => &extra_positions[k],
                    };
                    // `subarray` clamps both ends to the array
                    let lo = (c.first_triangle * 9).min(all.len());
                    let hi = ((c.first_triangle + c.source_faces.len()) * 9).min(all.len());
                    ComponentInfo {
                        settings: config
                            .map(|o| {
                                o.parts
                                    .iter()
                                    .filter(|p| {
                                        c.part_id.is_some()
                                            && p.attributes.get("id").map(String::as_str)
                                                == c.part_id.as_deref()
                                    })
                                    .cloned()
                                    .collect()
                            })
                            .unwrap_or_default(),
                        object_settings: config.map(|o| o.metadata.clone()).unwrap_or_default(),
                        positions: all[lo..hi.max(lo)].to_vec(),
                        path: c.path,
                        object_id: c.object_id,
                        part_id: c.part_id,
                        route: c.route,
                        kind: c.kind,
                        transform: c.transform,
                        corner_order: c.corner_order,
                        source: c.source,
                        first_triangle: c.first_triangle,
                        build_index: index,
                        unit_scale: scale,
                        source_faces: c.source_faces,
                    }
                })
                .collect()
        });
        let extras: Vec<ReadExtra> = e
            .extra
            .iter()
            .filter(|(_, s)| !s.out.is_empty())
            .map(|(k, _s)| {
                let (kind, name, settings) = match k {
                    RoleKey::Arena(i) => {
                        let r = &e.arena[*i];
                        (r.kind, r.name.clone(), r.settings.clone())
                    }
                    RoleKey::Vol(id, i) => {
                        let v = &e.prusa_vols[id.as_str()][*i];
                        (
                            v.kind,
                            v.name.clone(),
                            PartSettings {
                                prusa2: Some(v.settings.clone()),
                                ..Default::default()
                            },
                        )
                    }
                };
                ReadExtra {
                    kind,
                    name,
                    settings,
                    positions: extra_positions.swap_remove(k).unwrap(),
                }
            })
            .collect();
        objects.push(ThreeMfObject {
            name,
            tris: positions.len() / 9,
            meshes: stats.meshes,
            skipped: stats.skipped,
            dropped: stats.dropped,
            bbox: bbox(&positions),
            colors: (!col.paint.is_empty() || !col.rgb.is_empty()).then_some(col),
            extras,
            components,
            positions,
        });
    }

    if objects.is_empty() {
        return Err("this 3MF contains no printable mesh geometry".into());
    }

    let positions = if !merge {
        Vec::new()
    } else if opts.build_index.is_some() {
        objects[0].positions.clone()
    } else {
        merge_positions(&objects)
    };
    let session = read_session(parts);
    Ok(ThreeMf {
        unit,
        positions,
        meshes: agg_meshes,
        items: objects.len(),
        skipped: agg_skipped,
        dropped: agg_dropped,
        session_unreadable: session.is_none() && parts.contains_key(SESSION_PART),
        session,
        objects,
    })
}

/// `readSession` (web/threemf.js:1198-1207): SESSION_PART, parsed; `None` when absent or
/// unreadable (the file is still a part).
fn read_session(parts: &IndexMap<String, Vec<u8>>) -> Option<Json> {
    let data = parts.get(SESSION_PART)?;
    let s = json::parse(&decode(data))?;
    match s.get("v") {
        Some(Json::Num(v)) if *v == 1.0 => Some(s),
        _ => None,
    }
}

/// `mergePositions` (web/threemf.js:1210-1217): concatenate the per-object position arrays into
/// one merged `Float32Array` soup.
fn merge_positions(objects: &[ThreeMfObject]) -> Vec<f64> {
    use rayon::prelude::*;
    let mut out = vec![0.0f64; objects.iter().map(|o| o.positions.len()).sum()];
    let mut rest: &mut [f64] = &mut out;
    let mut parts: Vec<(&mut [f64], &[f64])> = Vec::with_capacity(objects.len());
    for o in objects {
        let (mine, tail) = rest.split_at_mut(o.positions.len());
        rest = tail;
        parts.push((mine, &o.positions));
    }
    parts.into_par_iter().for_each(|(dst, src)| {
        dst.par_chunks_mut(1 << 15)
            .zip(src.par_chunks(1 << 15))
            .for_each(|(d, s)| {
                for (d, &v) in d.iter_mut().zip(s) {
                    *d = (v as f32) as f64;
                }
            });
    });
    out
}
