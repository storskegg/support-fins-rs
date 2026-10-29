//! Port of `plugins/cli/project-analysis.js`. Analysis only: these meshes never replace the
//! project's original ZIP members.

use std::collections::HashSet;

use indexmap::IndexMap;
use sf_io::threemf::{
    PartKind, ReadExtra, ReadOpts, ThreeMfObject, XmlEvent, read_three_mf_parts, scan_xml,
};
use sf_io::zip::ProjectZip;

use crate::cut::cut_negatives;
use crate::project_xml::{Node, attr, child, children};

const ROOT: &str = "3D/3dmodel.model";
const CONFIG: &str = "Metadata/model_settings.config";

/// Port of `negativeComponents` (project-analysis.js:9-28): count occurrences, not IDs: a cutter
/// component may be instanced twice. The reader omits empty extras, so compare this count with the
/// decoded geometry.
pub fn negative_components(object: &Node, config: &Node) -> Result<usize, String> {
    let parts = children(config, "part");
    let negatives: Vec<&&Node> = parts
        .iter()
        .filter(|p| p.attr_exact("subtype") == Some("negative_part"))
        .collect();
    if negatives.is_empty() {
        return Ok(0);
    }
    let mut ids: HashSet<&str> = HashSet::new();
    let comps_parent = child(object, "components");
    let components: Vec<&Node> = comps_parent.map_or_else(Vec::new, |c| children(c, "component"));
    for part in negatives {
        let id = part.attr_exact("id").filter(|i| !i.is_empty());
        let unique = id.is_some_and(|i| {
            parts
                .iter()
                .filter(|p| p.attr_exact("id") == Some(i))
                .count()
                == 1
        });
        let Some(id) = id.filter(|_| unique) else {
            return Err("negative parts have missing or ambiguous part IDs".to_string());
        };
        if !components
            .iter()
            .any(|c| c.attr_exact("objectid") == Some(id))
        {
            return Err(format!("negative part {id} has no component"));
        }
        ids.insert(id);
    }
    let count = components
        .iter()
        .filter(|c| c.attr_exact("objectid").is_some_and(|o| ids.contains(o)))
        .count();
    if count == components.len() {
        return Err("negative parts leave no printable body components".to_string());
    }
    Ok(count)
}

/// What the seam in `subtractProjectNegatives` cuts with.
pub type Cutter<'a> = dyn Fn(&[f64], &[ReadExtra]) -> Result<crate::cut::CutResult, String> + 'a;

/// Port of `subtractProjectNegatives` (project-analysis.js:32-62): strict project policy; the
/// ordinary CLI/browser keep their existing fallback. `cut` is a private test seam for failures
/// that must never reach generation.
pub fn subtract_project_negatives(
    model: &ThreeMfObject,
    expected: usize,
    cut: &Cutter<'_>,
) -> Result<Vec<f64>, String> {
    if let Some(components) = &model.components {
        if components
            .iter()
            .any(|c| c.kind == "negative" && c.positions.is_empty())
        {
            return Err("negative parts contain missing or empty cutter geometry".to_string());
        }
        // One declared negative component can wrap several leaf meshes. Count root
        // component occurrences, not leaf meshes or mesh IDs reused at other poses.
        let occurrences: HashSet<Option<usize>> = components
            .iter()
            .filter(|c| c.kind == "negative" && !c.positions.is_empty())
            .map(|c| c.route.first().copied())
            .collect();
        if occurrences.len() != expected {
            return Err(format!(
                "negative parts: expected {expected} cutter components, decoded {} occurrences; missing or empty cutter geometry",
                occurrences.len()
            ));
        }
    }
    let negatives: Vec<ReadExtra> = model
        .extras
        .iter()
        .filter(|e| e.kind == PartKind::Negative)
        .cloned()
        .collect();
    if negatives.len() != expected {
        return Err(format!(
            "negative parts: expected {expected} cutter components, decoded {}; missing or empty cutter geometry",
            negatives.len()
        ));
    }
    if expected == 0 {
        return Ok(model.positions.clone()); // retain precision and geometry verbatim
    }
    // Paint need not be remapped onto temporary cut faces: the original painted meshes and
    // negative volumes are preserved in the output, unchanged.
    let result = cut(&model.positions, &negatives)
        .map_err(|e| format!("negative parts could not be subtracted: {e}"))?;
    if let Some(why) = result.why {
        let text = match why {
            "open" => "the printable mesh is not a closed solid",
            "whole" => "the cutters remove the entire printable mesh",
            "load" => "the subtraction engine could not load",
            other => other,
        };
        return Err(format!("negative parts could not be subtracted: {text}"));
    }
    if result.uncut != 0 || result.cut != expected {
        return Err(format!(
            "negative parts were not fully subtracted ({}/{expected} cutters); no fins generated for this object",
            result.cut
        ));
    }
    if result.positions.is_empty()
        || !result.positions.len().is_multiple_of(9)
        || !result.positions.iter().all(|v| v.is_finite())
    {
        return Err("negative parts produced empty or invalid analysis geometry".to_string());
    }
    Ok(result.positions)
}

/// The default seam: `cutNegatives` (`crate::cut`).
pub fn default_cut(
    positions: &[f64],
    negatives: &[ReadExtra],
) -> Result<crate::cut::CutResult, String> {
    Ok(cut_negatives(positions, negatives))
}

/// `partPath` (project-analysis.js:64-69).
fn part_path(path: &str) -> Result<String, String> {
    let name = path.strip_prefix('/').unwrap_or(path);
    if name.is_empty()
        || name.contains('\\')
        || name.contains([':', '?', '#', '%'])
        || name
            .split('/')
            .any(|p| p.is_empty() || p == ".." || p == ".")
    {
        return Err(format!("unsupported model path: {path}"));
    }
    Ok(name.to_string())
}

/// One selected object's identity for loading: its build number (1-based) and component list.
pub struct PositionsPlan<'a> {
    pub number: usize,
    pub object: &'a Node,
    pub negative_count: usize,
}

/// Port of `projectPositions` (project-analysis.js:73-103): load one selected dependency tree.
/// Returning only positions allows decoded members, paint and cutters to be collected before fin
/// generation / the next object.
pub fn project_positions(
    zip: &ProjectZip,
    root_bytes: &[u8],
    config_bytes: &[u8],
    plan: &PositionsPlan,
    cut: &Cutter<'_>,
) -> Result<Vec<f64>, String> {
    let mut members: IndexMap<String, Vec<u8>> = IndexMap::new();
    members.insert(ROOT.to_string(), root_bytes.to_vec());
    members.insert(CONFIG.to_string(), config_bytes.to_vec());
    let mut visiting: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();

    fn load(
        zip: &ProjectZip,
        name: &str,
        members: &mut IndexMap<String, Vec<u8>>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), String> {
        if visiting.contains(name) {
            return Err("cyclic model-part references".to_string());
        }
        if visited.contains(name) {
            return Ok(());
        }
        visiting.insert(name.to_string());
        if !members.contains_key(name) {
            members.insert(name.to_string(), zip.read(name)?);
        }
        let text = String::from_utf8_lossy(members.get(name).unwrap()).into_owned();
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();
        let mut paths: Vec<String> = Vec::new();
        scan_xml(&text, &mut |ev| {
            if let XmlEvent::Open {
                name: tag, attrs, ..
            } = ev
            {
                let local = tag.rsplit(':').next().unwrap_or(tag);
                if local == "model"
                    && let Some(unit) = attrs.get("unit").filter(|u| !u.is_empty())
                    && unit != "millimeter"
                {
                    return Err(format!("project mode requires millimeter units in {name}"));
                }
                if local == "component"
                    && let Some((_, path)) = attrs
                        .iter()
                        .find(|(k, _)| k.rsplit(':').next() == Some("path"))
                    && !path.is_empty()
                {
                    let p = part_path(path)?;
                    if !paths.contains(&p) {
                        paths.push(p);
                    }
                }
            }
            Ok(())
        })?;
        for p in paths {
            load(zip, &p, members, visiting, visited)?;
        }
        visiting.remove(name);
        visited.insert(name.to_string());
        Ok(())
    }

    let comps = child(plan.object, "components");
    for component in comps.map_or_else(Vec::new, |c| children(c, "component")) {
        let Some(path) = attr(component, "path").filter(|p| !p.is_empty()) else {
            return Err(
                "inline component meshes are not yet supported in project mode".to_string(),
            );
        };
        load(
            zip,
            &part_path(path)?,
            &mut members,
            &mut visiting,
            &mut visited,
        )?;
    }
    let parsed = read_three_mf_parts(
        &members,
        &ReadOpts {
            build_index: Some(plan.number - 1),
            double_precision: true,
            outward_winding: true,
            with_components: plan.negative_count > 0,
        },
    )?;
    let model = parsed.objects.into_iter().next().ok_or("no object")?;
    members.clear();
    subtract_project_negatives(&model, plan.negative_count, cut)
}
