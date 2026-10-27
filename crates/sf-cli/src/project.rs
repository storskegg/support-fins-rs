//! Port of `plugins/cli/project.js`: Bambu project augmentation. Original meshes are only read for
//! analysis: their bytes, painting and resource references never pass through a geometry writer.
//!
//! Random identifiers: `crypto.randomUUID()` names the support resources, so two runs differ in
//! those 36 characters (and the CRCs of the members holding them); `parity/cli/cli_parity.js`
//! compares outputs with every UUID replaced by its order of first appearance.

use std::collections::HashMap;

use indexmap::IndexMap;
use sf_io::json::{Json, parse};
use sf_io::threemf::mesh_xml;
use sf_io::zip::{ProjectZip, ZipData};

use crate::cli::{Args, FileResult, FinError, Io};
use crate::entry::{EngineOptions, compute_fins, options_from_dialog};
use crate::project_analysis::{PositionsPlan, default_cut, negative_components, project_positions};
use crate::project_cache::{CacheRequest, Generated, runtime_identity};
use crate::project_nozzle::project_nozzle;
use crate::project_report::{ProjectReport, ReportData, format_project_report};
use crate::project_xml::{
    Edit, Node, child, children, edit_xml, escape_xml, insert, meta, metadata, xml_tree,
};
use crate::uuid::random_uuid;

const ROOT: &str = "3D/3dmodel.model";
const CONFIG: &str = "Metadata/model_settings.config";
const SETTINGS: &str = "Metadata/project_settings.config";
const MARKER: &str = "Metadata/support_fins_project.json";
const SUPPORTS: &str = "3D/SupportFins/supports.model";
const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const PROD: &str = "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";
const REL: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const IDENTITY: [f64; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn identity_text() -> String {
    "1 0 0 0 1 0 0 0 1 0 0 0".to_string()
}

/// `enabled(v)` (project.js:19): `v === true || v === 1 || v === '1' || v === 'true'`.
fn enabled(v: Option<&Json>) -> bool {
    match v {
        Some(Json::Bool(true)) => true,
        Some(Json::Num(n)) => *n == 1.0,
        Some(Json::Str(s)) => s == "1" || s == "true",
        _ => false,
    }
}

/// `positive(v)` (project.js:20).
fn positive(v: Option<&Json>) -> bool {
    match v {
        None | Some(Json::Null) | Some(Json::Bool(_)) => false,
        Some(v) => {
            let s = v.to_js_string();
            if crate::js_trim(&s).is_empty() {
                return false;
            }
            let n = sf_engine::jsnum::string_to_number(&s);
            n.is_finite() && n > 0.0
        }
    }
}

/// `matrix(text)` (project.js:22-26).
fn matrix(text: Option<&str>) -> Result<[f64; 12], String> {
    let m: Vec<f64> = match text {
        Some(t) => t
            .split_whitespace()
            .map(sf_engine::jsnum::string_to_number)
            .collect(),
        None => IDENTITY.to_vec(),
    };
    if m.len() != 12 || m.iter().any(|v| !v.is_finite()) {
        return Err("invalid build transform".to_string());
    }
    let mut out = [0.0; 12];
    out.copy_from_slice(&m);
    Ok(out)
}

/// Port of `inverse` (project.js:28-37): row-vector inverse: convert the engine's world-mm
/// supports into the original build object's local frame. Its existing build transform is left
/// untouched.
fn inverse(m: &[f64; 12]) -> Result<[f64; 9], String> {
    let [a, b, c, d, e, f, g, h, i, ..] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < 1e-12 {
        return Err("singular build transform".to_string());
    }
    Ok([
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ]
    .map(|v| v / det))
}

/// Port of `supportMesh` (project.js:39-51).
fn support_mesh(tris: &[f32], offset: &sf_engine::types::Xyz, m: &[f64; 12]) -> Result<String, String> {
    let inv = inverse(m)?;
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut j = 0;
    while j + 2 < tris.len() {
        let x = tris[j] as f64 - offset.x - m[9];
        let y = tris[j + 1] as f64 - offset.y - m[10];
        let z = tris[j + 2] as f64 - offset.z - m[11];
        points.push([
            x * inv[0] + y * inv[3] + z * inv[6],
            x * inv[1] + y * inv[4] + z * inv[7],
            x * inv[2] + y * inv[5] + z * inv[8],
        ]);
        j += 3;
    }
    // A mirrored parent must also mirror the local winding to stay outward in world space.
    let determinant = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if determinant < 0.0 {
        let mut j = 0;
        while j + 2 < points.len() {
            points.swap(j + 1, j + 2);
            j += 3;
        }
    }
    Ok(mesh_xml(&points, None, None))
}

/// One build item (project.js:152-160).
struct Item<'a> {
    number: usize,
    id: String,
    item: &'a Node,
    config: Option<&'a Node>,
    object: Option<&'a Node>,
    instance: usize,
    name: String,
    printable: bool,
    plate: Option<String>,
}

/// Port of `pick` (project.js:53-64).
fn pick<'a, 'b>(items: &'b [&'b Item<'a>], selectors: &[String]) -> Result<Vec<&'b Item<'a>>, String> {
    if selectors.is_empty() {
        return Ok(items.iter().copied().filter(|o| o.printable).collect());
    }
    let mut chosen: Vec<usize> = Vec::new();
    for s in selectors {
        let numeric = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let matches: Vec<usize> = (0..items.len())
            .filter(|&k| {
                if numeric {
                    items[k].number as f64 == sf_engine::jsnum::string_to_number(s)
                } else {
                    items[k].name == *s
                }
            })
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "object {} is {}; use --list-objects",
                Json::Str(s.clone()).stringify(),
                if matches.is_empty() { "missing" } else { "ambiguous" }
            ));
        }
        if !items[matches[0]].printable {
            return Err(format!("object {s} is not printable"));
        }
        if !chosen.contains(&matches[0]) {
            chosen.push(matches[0]);
        }
    }
    Ok((0..items.len()).filter(|k| chosen.contains(k)).map(|k| items[k]).collect())
}

fn md<'a>(m: &'a IndexMap<String, Option<String>>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(|v| v.as_deref())
}

fn is_normal_part(p: &Node) -> bool {
    match p.attr_exact("subtype") {
        Some("normal_part") | None => true,
        Some(s) => s.is_empty(),
    }
}

fn str_json(v: Option<&str>) -> Option<Json> {
    v.map(|s| Json::Str(s.to_string()))
}

/// Port of `regularHeight` (project.js:66-79).
fn regular_height(
    settings: &Json,
    config: &Node,
    explicit: Option<&Json>,
    variable: bool,
) -> Result<Json, String> {
    if let Some(e) = explicit {
        return Ok(e.clone());
    }
    if variable {
        return Err("variable layer heights need an explicit --layer-height for fin generation".to_string());
    }
    let own = metadata(config);
    let base: Option<Json> = str_json(md(&own, "layer_height")).or_else(|| settings.get("layer_height").cloned());
    let mut heights: Vec<Option<Json>> = children(config, "part")
        .into_iter()
        .filter(|p| is_normal_part(p))
        .map(|p| {
            let m = metadata(p);
            str_json(md(&m, "layer_height")).or_else(|| base.clone())
        })
        .collect();
    if heights.is_empty() {
        heights.push(base);
    }
    let nums: Vec<f64> = heights
        .iter()
        .map(|h| h.as_ref().map_or(f64::NAN, |j| sf_engine::jsnum::string_to_number(&j.to_js_string())))
        .collect();
    let mut unique: Vec<f64> = Vec::new();
    for n in &nums {
        if !unique.iter().any(|u| u == n || (u.is_nan() && n.is_nan())) {
            unique.push(*n);
        }
    }
    if heights.iter().any(|h| !positive(h.as_ref())) || unique.len() != 1 {
        return Err("missing or conflicting regular layer heights; supply --layer-height".to_string());
    }
    Ok(Json::Num(nums[0]))
}

/// Port of `filament` (project.js:81-99).
fn filament(settings: &Json, config: &Node, explicit: Option<f64>) -> Result<f64, String> {
    let count = match settings.get("filament_colour") {
        Some(Json::Arr(a)) => a.len(),
        _ => 0,
    };
    let own = metadata(config);
    let num = |v: Option<&str>| v.map_or(f64::NAN, sf_engine::jsnum::string_to_number);
    let mut n = explicit;
    if n.is_none() {
        // Zero means inherited. Prefer the object's base, not a painted triangle.
        let e = num(md(&own, "extruder"));
        n = if e > 0.0 { Some(e) } else { None };
        if n.is_none() {
            let mut unique: Vec<f64> = Vec::new();
            for p in children(config, "part").into_iter().filter(|p| is_normal_part(p)) {
                let v = num(md(&metadata(p), "extruder"));
                if v > 0.0 && !unique.contains(&v) {
                    unique.push(v);
                }
            }
            if unique.len() == 1 {
                n = Some(unique[0]);
            } else if unique.is_empty() && count == 1 {
                n = Some(1.0);
            }
        }
    }
    match n {
        Some(v) if v.fract() == 0.0 && v >= 1.0 && v <= count as f64 => Ok(v),
        _ => Err(format!("cannot resolve an existing filament slot (1..{count}); supply --fin-extruder")),
    }
}

/// Port of `setMetadata` (project.js:101-106).
fn set_metadata(node: &Node, key: &str, value: &str, edits: &mut Vec<Edit>, appended: &mut Vec<String>) -> Result<(), String> {
    let existing: Vec<&Node> = children(node, "metadata")
        .into_iter()
        .filter(|m| m.attr_exact("key") == Some(key))
        .collect();
    if existing.len() > 1 {
        return Err(format!("duplicate {key} setting"));
    }
    if let Some(e) = existing.first() {
        edits.push((e.start, e.end, meta(key, value)));
    } else {
        appended.push(meta(key, value));
    }
    Ok(())
}

/// Port of `finProject` (project.js:108-119).
pub fn fin_project(input: &str, args: &Args, io: &dyn Io) -> Result<FileResult, FinError> {
    let mut report = ProjectReport::new(args.check);
    match process_project(input, args, io, &mut report) {
        Ok(mut result) => {
            if args.list_objects || args.list_plates {
                return Ok(result);
            }
            let data = report.data();
            let text = format!("{}\n{}", result.report, format_project_report(&data));
            // `{ ...result, ...data, report }`: `report` keeps its place, the data follows
            if let Json::Obj(fields) = &mut result.json {
                for (k, v) in data.to_json_fields() {
                    fields.push((k, v));
                }
                if let Some(slot) = fields.iter_mut().find(|(k, _)| k == "report") {
                    slot.1 = Json::Str(text.clone());
                }
            }
            result.report = text;
            result.incompatible = data.summary_count("incompatible") != 0;
            Ok(result)
        }
        Err(message) => Err(FinError {
            message,
            report: if args.list_objects || args.list_plates { None } else { Some(report.data()) },
        }),
    }
}

/// Everything `processProject` keeps for one planned object (project.js:218-247).
struct Plan<'a, 'b> {
    o: &'b Item<'a>,
    transform: [f64; 12],
    height: Json,
    slot: f64,
    nozzle: f64,
    nozzle_source: &'static str,
    options: EngineOptions,
    native: bool,
    by_object: bool,
    negative_count: usize,
}

#[allow(clippy::too_many_lines)]
fn process_project(
    input: &str,
    args: &Args,
    io: &dyn Io,
    report: &mut ProjectReport,
) -> Result<FileResult, String> {
    if !input.to_ascii_lowercase().ends_with(".3mf") {
        return Err("--preserve-project reads a Bambu Studio/MakerWorld .3mf".to_string());
    }
    let output = args.output.clone().unwrap_or_else(|| {
        // input.replace(/\.3mf$/i, '-fins-project.3mf')
        format!("{}-fins-project.3mf", &input[..input.len() - 4])
    });
    if !output.to_ascii_lowercase().ends_with(".3mf") {
        return Err("--preserve-project writes a .3mf".to_string());
    }
    let listing = args.list_objects || args.list_plates;
    if !listing && !args.check {
        if !io.has_project_io() {
            return Err("project output requires an atomic file-system writer".to_string());
        }
        // In a batch, this output may alias a different input (including by hard link).
        // --overwrite authorizes replacing old outputs, never any of the source files.
        for source in &args.inputs {
            io.check_project_output(source, &output, args.overwrite)?;
        }
    }
    let zip = ProjectZip::new(&io.read(input)?)?;
    if zip.entries.contains_key(MARKER) || zip.entries.contains_key(SUPPORTS) {
        return Err("this project already has generated fins; use the original project".to_string());
    }
    // The layout, not the application name, is what this edits: Bambu Studio and its forks
    // (OrcaSlicer, QIDI Studio, ...) all write these members and plates.
    if ![ROOT, CONFIG, SETTINGS].iter().all(|n| zip.entries.contains_key(*n)) {
        let missing: Vec<&str> = [CONFIG, SETTINGS].into_iter().filter(|n| !zip.entries.contains_key(*n)).collect();
        return Err(format!(
            "--preserve-project needs a Bambu Studio-style project (Bambu Studio, OrcaSlicer, QIDI Studio): {} missing",
            if missing.is_empty() { ROOT.to_string() } else { missing.join(", ") }
        ));
    }
    let (root_xml, config_xml, settings_text) = (zip.text(ROOT)?, zip.text(CONFIG)?, zip.text(SETTINGS)?);
    let root = xml_tree(&root_xml)?;
    let config = xml_tree(&config_xml)?;
    if root.attr_exact("unit") != Some("millimeter") {
        return Err("project mode currently requires millimeter units".to_string());
    }
    let settings = parse(&settings_text).ok_or("Unexpected token in JSON")?;
    let (Some(resources), Some(build)) = (child(&root, "resources"), child(&root, "build")) else {
        return Err("missing project resources/build".to_string());
    };
    let res_objects = children(resources, "object");
    let mut objects: IndexMap<String, &Node> = IndexMap::new();
    for o in &res_objects {
        objects.insert(o.attr_exact("id").unwrap_or("undefined").to_string(), o);
    }
    let cfg_objects = children(&config, "object");
    let mut configs: IndexMap<String, &Node> = IndexMap::new();
    for o in &cfg_objects {
        configs.insert(o.attr_exact("id").unwrap_or("undefined").to_string(), o);
    }
    if objects.len() != res_objects.len() || configs.len() != cfg_objects.len() {
        return Err("duplicate project object IDs".to_string());
    }
    let plates = children(&config, "plate");
    if plates.is_empty() {
        return Err("no plates in the project settings; save it from Bambu Studio, OrcaSlicer or QIDI Studio".to_string());
    }
    // membership: "object:instance" -> (plate id, plate index)
    let mut membership: HashMap<String, (String, usize)> = HashMap::new();
    // plateCounts: id -> (objects, printableObjects), in first-seen order
    let mut plate_counts: IndexMap<String, (usize, usize)> = IndexMap::new();
    for (i, p) in plates.iter().enumerate() {
        let pm = metadata(p);
        let id = md(&pm, "plater_id").map_or_else(|| (i + 1).to_string(), str::to_string);
        // An empty or repeated ID only matters where plates are picked or listed by it; plain
        // generation ran on such projects before --plate and still does.
        if crate::js_trim(&id).is_empty() || plate_counts.contains_key(&id) {
            if listing || !args.plates.is_empty() {
                return Err(format!(
                    "empty or duplicate plate ID {}; save distinct plates in the slicer",
                    Json::Str(id).stringify()
                ));
            }
        } else {
            plate_counts.insert(id.clone(), (0, 0));
        }
        for instance in children(p, "model_instance") {
            let m = metadata(instance);
            let key = format!(
                "{}:{}",
                md(&m, "object_id").unwrap_or("undefined"),
                md(&m, "instance_id").unwrap_or("undefined")
            );
            if membership.contains_key(&key) {
                return Err("ambiguous plate membership".to_string());
            }
            membership.insert(key, (id.clone(), i));
        }
    }
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    let build_items = children(build, "item");
    let mut items: Vec<Item> = Vec::new();
    for (index, item) in build_items.iter().enumerate() {
        let id = item.attr_exact("objectid").unwrap_or("undefined").to_string();
        let instance = *occurrences.get(&id).unwrap_or(&0);
        occurrences.insert(id.clone(), instance + 1);
        let cfg = configs.get(&id).copied();
        let object = objects.get(&id).copied();
        let fallback = format!("Object {}", index + 1);
        let name = match cfg {
            Some(c) => {
                let m = metadata(c);
                md(&m, "name")
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .or_else(|| object.and_then(|o| o.attr_exact("name")).filter(|n| !n.is_empty()).map(str::to_string))
                    .unwrap_or(fallback)
            }
            None => fallback,
        };
        items.push(Item {
            number: index + 1,
            plate: membership.get(&format!("{id}:{instance}")).map(|(p, _)| p.clone()),
            id,
            item,
            config: cfg,
            object,
            instance,
            name,
            printable: !matches!(item.attr_exact("printable"), Some("0") | Some("false")),
        });
    }
    for item in &items {
        if let Some(plate) = item.plate.as_ref().and_then(|p| plate_counts.get_mut(p)) {
            plate.0 += 1;
            if item.printable {
                plate.1 += 1;
            }
        }
    }
    let mut requested: Vec<String> = Vec::new();
    for p in &args.plates {
        if !requested.contains(p) {
            requested.push(p.clone());
        }
    }
    for id in &requested {
        let Some(plate) = plate_counts.get(id) else {
            return Err(format!("plate {} is missing; use --list-plates", Json::Str(id.clone()).stringify()));
        };
        if !listing && plate.1 == 0 {
            return Err(format!(
                "plate {} has no printable objects; use --list-plates",
                Json::Str(id.clone()).stringify()
            ));
        }
    }
    // Keep original build ordinals: filtering must not change --object numbers or the buildIndex
    // used by the mesh reader. Names resolve inside these plates.
    let in_plates: Vec<&Item> = items
        .iter()
        .filter(|o| requested.is_empty() || o.plate.as_ref().is_some_and(|p| requested.contains(p)))
        .collect();
    if listing {
        let summaries: Vec<(&String, &(usize, usize))> = plate_counts
            .iter()
            .filter(|(id, _)| requested.is_empty() || requested.contains(id))
            .collect();
        let mut lines: Vec<String> = summaries
            .iter()
            .map(|(id, (o, p))| format!("Plate {id}: {o} objects, {p} printable"))
            .collect();
        let mut fields: Vec<(String, Json)> = vec![
            ("input".into(), Json::Str(input.to_string())),
            ("output".into(), Json::Null),
            (
                "plates".into(),
                Json::Arr(
                    summaries
                        .iter()
                        .map(|(id, (o, p))| {
                            Json::Obj(vec![
                                ("id".into(), Json::Str((*id).clone())),
                                ("objects".into(), Json::Num(*o as f64)),
                                ("printableObjects".into(), Json::Num(*p as f64)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ];
        if args.list_objects {
            fields.push((
                "objects".into(),
                Json::Arr(
                    in_plates
                        .iter()
                        .map(|o| {
                            Json::Obj(vec![
                                ("number".into(), Json::Num(o.number as f64)),
                                ("id".into(), Json::Str(o.id.clone())),
                                ("name".into(), Json::Str(o.name.clone())),
                                ("printable".into(), Json::Bool(o.printable)),
                                ("plate".into(), o.plate.clone().map_or(Json::Null, Json::Str)),
                            ])
                        })
                        .collect(),
                ),
            ));
            for o in &in_plates {
                lines.push(format!(
                    "{}: {} (resource {}, plate {}, {})",
                    o.number,
                    o.name,
                    o.id,
                    o.plate.as_deref().unwrap_or("?"),
                    if o.printable { "printable" } else { "not printable" }
                ));
            }
        }
        let text = lines.join("\n");
        fields.push(("report".into(), Json::Str(text.clone())));
        return Ok(FileResult {
            input: input.to_string(),
            output: None,
            report: text,
            json: Json::Obj(fields),
            incompatible: false,
        });
    }
    let selected = pick(&in_plates, &args.objects)?;
    report.select(
        &selected
            .iter()
            .map(|o| (o.number, o.id.clone(), o.name.clone(), o.plate.clone()))
            .collect::<Vec<_>>(),
    );
    if selected.is_empty() {
        return Err("no printable objects selected".to_string());
    }
    let warn = |s: &str| io.err(&format!("support-fins: {input}: {s}"));
    // Objects named with --object must all work; in the every-object default a model this mode
    // can't handle is left as it was, with a warning, so the rest get fins.
    let skip_or_throw = |report: &mut ProjectReport, o: &Item, e: &str| -> Result<(), String> {
        let message = format!("object {} ({}): {e}", o.number, o.name);
        let r = report.object(o.number);
        r.set("reason", Json::Str(e.to_string()));
        r.set(
            "status",
            Json::Str(
                if args.check {
                    "incompatible"
                } else if !args.objects.is_empty() {
                    "failed"
                } else {
                    "skipped"
                }
                .to_string(),
            ),
        );
        if args.check {
            return Ok(());
        }
        if !args.objects.is_empty() {
            return Err(message);
        }
        warn(&format!("skipped {message}"));
        Ok(())
    };
    // Variable layer heights are per object, numbered by object order (1-based), not resource ID:
    // Bambu renumbers resources when it saves and keeps these numbers.
    let mut variable_ids: Vec<f64> = Vec::new();
    for name in zip.entries.keys() {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with("layer_heights_profile.txt") {
            // /^object_id=(\d+)\|/gm
            for line in zip.text(name)?.split(['\n', '\r']) {
                if let Some(rest) = line.strip_prefix("object_id=")
                    && let Some((digits, _)) = rest.split_once('|')
                    && !digits.is_empty()
                    && digits.bytes().all(|b| b.is_ascii_digit())
                {
                    variable_ids.push(sf_engine::jsnum::string_to_number(digits));
                }
            }
        } else if lower.ends_with("layer_config_ranges.xml") {
            let tree = xml_tree(&zip.text(name)?)?;
            for o in children(&tree, "object") {
                variable_ids.push(o.attr_exact("id").map_or(f64::NAN, sf_engine::jsnum::string_to_number));
            }
        }
    }
    let order: HashMap<String, usize> = res_objects
        .iter()
        .enumerate()
        .map(|(i, o)| (o.attr_exact("id").unwrap_or("undefined").to_string(), i + 1))
        .collect();
    let mut plans: Vec<Plan> = Vec::new();
    for &o in &selected {
        let planned = (|| -> Result<Plan, String> {
            let (Some(object), Some(cfg), Some(_plate)) = (o.object, o.config, o.plate.as_ref()) else {
                return Err("missing object settings or plate assignment".to_string());
            };
            if occurrences.get(&o.id) != Some(&1) {
                return Err("shared build instances are not supported yet; make each instance independent in the slicer".to_string());
            }
            if child(object, "components").is_none() || child(object, "mesh").is_some() {
                return Err("expected a Bambu component assembly".to_string());
            }
            let negative_count = negative_components(object, cfg)?;
            let transform = matrix(o.item.attr_exact("transform"))?;
            inverse(&transform)?;
            let variable = order.get(&o.id).is_some_and(|&n| variable_ids.contains(&(n as f64)));
            let height = regular_height(&settings, cfg, args.dialog_get("layerHeight"), variable)?;
            let slot = filament(&settings, cfg, args.fin_extruder)?;
            let nozzle = project_nozzle(&settings, args.dialog_get("nozzle"))?;
            let mut dialog = args.dialog.clone();
            let mut set = |k: &str, v: Json| match dialog.iter_mut().find(|(n, _)| n == k) {
                Some(slot) => slot.1 = v,
                None => dialog.push((k.to_string(), v)),
            };
            set("layerHeight", height.clone());
            set("nozzle", Json::Num(nozzle.nozzle));
            let options = options_from_dialog(&dialog)?;
            let own = metadata(cfg);
            let native = enabled(str_json(md(&own, "enable_support")).or_else(|| settings.get("enable_support").cloned()).as_ref())
                || children(cfg, "part")
                    .into_iter()
                    .any(|p| enabled(str_json(md(&metadata(p), "enable_support")).as_ref()));
            // By-object printing finishes one object before the next starts, so the fins can't be
            // their own object there: they become a part of the model. A plate can override the
            // project's print sequence.
            let plate_index = membership[&format!("{}:{}", o.id, o.instance)].1;
            let pm = metadata(plates[plate_index]);
            let sequence = str_json(md(&pm, "print_sequence")).or_else(|| settings.get("print_sequence").cloned());
            let by_object = sequence
                .as_ref()
                .map_or(String::new(), Json::to_js_string)
                .to_ascii_lowercase()
                .contains("object");
            if negative_count != 0 && by_object {
                return Err("negative parts require print-by-layer; print-by-object is not supported for them yet".to_string());
            }
            let result = report.object(o.number);
            result.set("nozzle", Json::Num(nozzle.nozzle));
            result.set("nozzleSource", Json::Str(nozzle.nozzle_source.to_string()));
            if nozzle.nozzle_source == "override" {
                result.warn("nozzle override applies to fin generation only; saved printer settings are unchanged");
            }
            if let Some(w) = &nozzle.warning {
                result.warn(w);
            }
            if native && !args.disable_native_supports {
                result.warn("native slicer supports remain enabled; both native supports and fins may print");
            }
            if by_object {
                result.warn("prints by object: any generated fins become a part of this model, so their tines weld to it");
            }
            if args.disable_native_supports
                && children(cfg, "metadata").iter().filter(|m| m.attr_exact("key") == Some("enable_support")).count() > 1
            {
                return Err("duplicate enable_support setting".to_string());
            }
            Ok(Plan {
                o,
                transform,
                height,
                slot,
                nozzle: nozzle.nozzle,
                nozzle_source: nozzle.nozzle_source,
                options,
                native,
                by_object,
                negative_count,
            })
        })();
        match planned {
            Ok(p) => plans.push(p),
            Err(e) => skip_or_throw(report, o, &e)?,
        }
    }
    if plans.is_empty() && !args.check {
        return Err("none of the selected objects can take fins in project mode (see the warnings above)".to_string());
    }
    if !args.check && plans.iter().any(|p| p.native) && !args.disable_native_supports {
        warn("native slicer supports remain enabled; both native supports and fins may print. Use --disable-native-supports to turn them off on selected objects.");
    }
    if args.resume && !io.has_project_cache() {
        return Err("--resume requires a file-system checkpoint store".to_string());
    }
    let mut cache = if args.resume {
        let dialog = Json::Obj(args.dialog.clone());
        Some(io.open_project_cache(
            &CacheRequest {
                input,
                bytes: &zip.bytes,
                selection_objects: selected.iter().map(|o| o.number).collect(),
                selection_explicit: !args.objects.is_empty(),
                settings: Json::Obj(vec![
                    ("dialog".into(), dialog),
                    ("finExtruder".into(), args.fin_extruder.map_or(Json::Null, Json::Num)),
                    ("disableNativeSupports".into(), Json::Bool(args.disable_native_supports)),
                ]),
                cache_dir: args.cache_dir.as_deref(),
            },
            &warn,
        )?)
    } else {
        None
    };
    let mut root_edits: Vec<Edit> = Vec::new();
    let mut config_edits: Vec<Edit> = Vec::new();
    let mut support_objects: Vec<String> = Vec::new();
    let mut results: Vec<Json> = Vec::new();
    let mut new_resources: Vec<String> = Vec::new();
    let mut new_items: Vec<String> = Vec::new();
    // plate index -> instances, in first-seen order
    let mut plate_instances: IndexMap<usize, Vec<String>> = IndexMap::new();
    let mut next_identify = 1.0
        + plates
            .iter()
            .flat_map(|p| children(p, "model_instance"))
            .filter_map(|i| {
                let n = md(&metadata(i), "identify_id").map(sf_engine::jsnum::string_to_number);
                n.filter(|n| n.is_finite())
            })
            .fold(0.0, f64::max);
    let mut max_id = 0.0f64;
    let ids: Vec<String> = objects
        .keys()
        .cloned()
        .chain(
            children(&config, "object")
                .into_iter()
                .flat_map(|o| children(o, "part").into_iter().map(|p| p.attr_exact("id").unwrap_or("undefined").to_string())),
        )
        .collect();
    for id in ids {
        let digits = !id.is_empty() && id.as_bytes()[0] != b'0' && id.bytes().all(|b| b.is_ascii_digit());
        let n = sf_engine::jsnum::string_to_number(&id);
        if !digits || n > 9007199254740991.0 {
            return Err("invalid resource ID".to_string());
        }
        max_id = max_id.max(n);
    }
    let mut next_id = max_id + 1.0;
    let mut prefix: Option<String> = root
        .attrs
        .iter()
        .find(|(k, v)| k.starts_with("xmlns:") && v == PROD)
        .map(|(k, _)| k[6..].to_string());
    if prefix.is_none() {
        if root.attr_exact("xmlns:p").is_some() {
            return Err("unsupported production namespace".to_string());
        }
        prefix = Some("p".to_string());
        root_edits.push((root.open - 1, root.open - 1, format!(" xmlns:p=\"{PROD}\"")));
    }
    let prefix = prefix.unwrap();
    // Load only this object's dependency tree. Discard the inflated meshes and parsed geometry
    // before the next object, instead of retaining an entire plate.
    for p in &plans {
        if !args.check {
            warn(&format!(
                "object {}/{}, plate {}: {}; layer {} mm, filament {}, nozzle {} mm ({})",
                p.o.number,
                items.len(),
                p.o.plate.as_deref().unwrap_or("undefined"),
                p.o.name,
                p.height.to_js_string(),
                sf_engine::jsnum::to_js_string(p.slot),
                sf_engine::jsnum::to_js_string(p.nozzle),
                p.nozzle_source
            ));
        }
        let mut generated: Option<Generated> = cache.as_ref().and_then(|c| c.load(p.o.number, &warn));
        let reused = generated.is_some();
        let attempt = (|| -> Result<Option<()>, String> {
            if generated.is_none() {
                let positions = project_positions(
                    &zip,
                    root_xml.as_bytes(),
                    config_xml.as_bytes(),
                    &PositionsPlan { number: p.o.number, object: p.o.object.unwrap(), negative_count: p.negative_count },
                    &|pos, extras| default_cut(pos, extras),
                )?;
                if args.check {
                    let r = report.object(p.o.number);
                    r.set("status", Json::Str("compatible".into()));
                    r.set("layerHeight", p.height.clone());
                    r.set("filament", Json::Num(p.slot));
                    return Ok(None);
                }
                let r = compute_fins(&positions, &p.options)?;
                generated = Some(Generated { triangles: r.triangles, offset: r.offset, stats: r.stats.to_json() });
            }
            let g = generated.as_ref().unwrap();
            // Gather this object's edits and commit them only once nothing can throw, so a
            // skipped object leaves the project exactly as it was.
            let mut appended: Vec<String> = Vec::new();
            let (mut o_root, mut o_config, mut o_supports): (Vec<Edit>, Vec<Edit>, Vec<String>) = (vec![], vec![], vec![]);
            let (mut o_resources, mut o_items, mut o_instances): (Vec<String>, Vec<String>, Vec<String>) = (vec![], vec![], vec![]);
            let mut support_id: Option<String> = None;
            let (mut id_ctr, mut ident_ctr) = (next_id, next_identify);
            if !g.triangles.is_empty() {
                let id = sf_engine::jsnum::to_js_string(id_ctr);
                id_ctr += 1.0;
                let name = format!("{} supports", p.o.name);
                let mesh = support_mesh(&g.triangles, &g.offset, &p.transform)?;
                o_supports.push(format!(
                    "<object id=\"{id}\" type=\"model\" name=\"{}\" p:UUID=\"{}\">{mesh}</object>",
                    escape_xml(&name),
                    random_uuid()
                ));
                let component = format!(
                    "<component objectid=\"{id}\" {prefix}:path=\"/{SUPPORTS}\" {prefix}:UUID=\"{}\" transform=\"{}\"/>",
                    random_uuid(),
                    identity_text()
                );
                let part = format!(
                    "<part id=\"{id}\" subtype=\"normal_part\">{}{}{}</part>",
                    meta("name", &name),
                    meta("extruder", &sf_engine::jsnum::to_js_string(p.slot)),
                    meta("matrix", "1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1")
                );
                if p.by_object {
                    warn(&format!(
                        "object {} ({}) prints by object: its fins are a part of it, so their tines weld to it",
                        p.o.number, p.o.name
                    ));
                    o_root.push(insert(child(p.o.object.unwrap(), "components").unwrap(), &component));
                    appended.push(part);
                } else {
                    // Their own object, as the site's 3MF does: its own perimeters, so a tine
                    // only touches the part. Same build transform and plate, so it opens in
                    // register.
                    let sid = sf_engine::jsnum::to_js_string(id_ctr);
                    id_ctr += 1.0;
                    support_id = Some(sid.clone());
                    o_resources.push(format!(
                        "<object id=\"{sid}\" {prefix}:UUID=\"{}\" type=\"model\"><components>{component}</components></object>",
                        random_uuid()
                    ));
                    o_items.push(format!(
                        "<item objectid=\"{sid}\" {prefix}:UUID=\"{}\" transform=\"{}\" printable=\"1\"/>",
                        random_uuid(),
                        escape_xml(p.o.item.attr_exact("transform").unwrap_or(&identity_text()))
                    ));
                    let cfg = p.o.config.unwrap();
                    o_config.push((
                        cfg.end,
                        cfg.end,
                        format!(
                            "<object id=\"{sid}\">{}{}{}{part}</object>",
                            meta("name", &name),
                            meta("extruder", &sf_engine::jsnum::to_js_string(p.slot)),
                            meta("enable_support", "0")
                        ),
                    ));
                    o_instances.push(format!(
                        "<model_instance>{}{}{}</model_instance>",
                        meta("object_id", &sid),
                        meta("instance_id", "0"),
                        meta("identify_id", &sf_engine::jsnum::to_js_string(ident_ctr))
                    ));
                    ident_ctr += 1.0;
                }
            }
            if args.disable_native_supports {
                let cfg = p.o.config.unwrap();
                set_metadata(cfg, "enable_support", "0", &mut o_config, &mut appended)?;
                for part in children(cfg, "part") {
                    for m in children(part, "metadata").into_iter().filter(|m| m.attr_exact("key") == Some("enable_support")) {
                        o_config.push((m.start, m.end, meta("enable_support", "0")));
                    }
                }
            }
            if !appended.is_empty() {
                o_config.push(insert(p.o.config.unwrap(), &appended.concat()));
            }
            root_edits.extend(o_root);
            config_edits.extend(o_config);
            support_objects.extend(o_supports);
            new_resources.extend(o_resources);
            new_items.extend(o_items);
            next_id = id_ctr;
            next_identify = ident_ctr;
            if !o_instances.is_empty() {
                let plate_index = membership[&format!("{}:{}", p.o.id, p.o.instance)].1;
                plate_instances.entry(plate_index).or_default().extend(o_instances);
            }
            let tris = g.triangles.len() as f64 / 9.0;
            results.push(Json::Obj(vec![
                ("number".into(), Json::Num(p.o.number as f64)),
                ("id".into(), Json::Str(p.o.id.clone())),
                ("name".into(), Json::Str(p.o.name.clone())),
                ("plate".into(), p.o.plate.clone().map_or(Json::Null, Json::Str)),
                ("supportObject".into(), support_id.map_or(Json::Null, Json::Str)),
                ("nozzle".into(), Json::Num(p.nozzle)),
                ("nozzleSource".into(), Json::Str(p.nozzle_source.to_string())),
                ("layerHeight".into(), p.height.clone()),
                ("filament".into(), Json::Num(p.slot)),
                ("triangles".into(), Json::Num(tris)),
                ("stats".into(), g.stats.clone()),
            ]));
            report.object(p.o.number).set(
                "status",
                Json::Str(if g.triangles.is_empty() { "no_supports" } else { "processed" }.to_string()),
            );
            Ok(Some(()))
        })();
        match attempt {
            Ok(None) => continue, // --check
            Ok(Some(())) => {}
            Err(e) => {
                skip_or_throw(report, p.o, &e)?;
                continue;
            }
        }
        // A cache write failure must abort even in the default skip-on-object-error mode. Save
        // only after all of this object's edits have succeeded.
        if let (Some(c), false) = (cache.as_mut(), reused) {
            c.save(p.o.number, generated.as_ref().unwrap())?;
        }
    }
    if args.check {
        // These small manifests are needed when writing a project. Check their readability too,
        // without constructing geometry or an output archive.
        xml_tree(&zip.text("[Content_Types].xml")?)?;
        for name in zip.entries.keys() {
            if name.ends_with(".rels") {
                xml_tree(&zip.text(name)?)?;
            }
        }
        return Ok(FileResult {
            input: input.to_string(),
            output: None,
            report: "Project compatibility check; generation and slicing have not been tested.".to_string(),
            json: Json::Obj(vec![
                ("input".into(), Json::Str(input.to_string())),
                ("output".into(), Json::Null),
                ("objects".into(), Json::Arr(vec![])),
                (
                    "report".into(),
                    Json::Str("Project compatibility check; generation and slicing have not been tested.".into()),
                ),
            ]),
            incompatible: false,
        });
    }
    if results.is_empty() {
        return Err("no objects could be finned (see the warnings above)".to_string());
    }
    if !new_resources.is_empty() {
        root_edits.push(insert(resources, &new_resources.concat()));
        root_edits.push(insert(build, &new_items.concat()));
    }
    for (plate, instances) in &plate_instances {
        config_edits.push(insert(plates[*plate], &instances.concat()));
    }
    // changes: an ordered map name -> new content (None deletes)
    let mut changes: Vec<(String, Option<ZipData>)> = Vec::new();
    fn set_change(changes: &mut Vec<(String, Option<ZipData>)>, name: &str, v: Option<ZipData>) {
        match changes.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = v,
            None => changes.push((name.to_string(), v)),
        }
    }
    // Slicing caches cannot describe the augmented model. Keep custom_gcode_per_layer.xml: it
    // contains user-authored pauses/filament changes, not the generated toolpath.
    let mut stale: Vec<String> = zip
        .entries
        .keys()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".gcode")
                || l.ends_with(".bgcode")
                || l == "metadata/slice_info.config"
                || (l.starts_with("metadata/plate_") && l.ends_with(".json") && l["metadata/plate_".len()..l.len() - 5].bytes().all(|b| b.is_ascii_digit()) && l.len() > "metadata/plate_.json".len())
        })
        .cloned()
        .collect();
    const PLATE_KEYS: [&str; 7] = [
        "gcode_file",
        "thumbnail_file",
        "thumbnail_no_light_file",
        "top_file",
        "pick_file",
        "slice_prediction",
        "slice_weight",
    ];
    for plate in &plates {
        for m in children(plate, "metadata") {
            if m.attr_exact("key").is_some_and(|k| PLATE_KEYS.contains(&k)) {
                if let Some(v) = m.attr_exact("value").filter(|v| !v.is_empty())
                    && zip.entries.contains_key(v)
                    && !stale.iter().any(|s| s == v)
                {
                    stale.push(v.to_string());
                }
                config_edits.push((m.start, m.end, String::new()));
            }
        }
    }
    for name in &stale {
        set_change(&mut changes, name, None);
    }
    set_change(&mut changes, ROOT, Some(ZipData::Text(edit_xml(&root_xml, root_edits)?)));
    set_change(&mut changes, CONFIG, Some(ZipData::Text(edit_xml(&config_xml, config_edits)?)));
    set_change(
        &mut changes,
        MARKER,
        Some(ZipData::Text(
            Json::Obj(vec![
                ("version".into(), Json::Num(1.0)),
                ("objects".into(), Json::Arr(results.clone())),
                ("needsReslice".into(), Json::Bool(true)),
            ])
            .stringify(),
        )),
    );
    if !support_objects.is_empty() {
        set_change(
            &mut changes,
            SUPPORTS,
            Some(ZipData::Text(format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><model unit=\"millimeter\" xmlns=\"{CORE}\" xmlns:p=\"{PROD}\"><resources>{}</resources></model>",
                support_objects.concat()
            ))),
        );
        let name = "3D/_rels/3dmodel.model.rels";
        let text = if zip.entries.contains_key(name) {
            zip.text(name)?
        } else {
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".to_string()
        };
        let node = xml_tree(&text)?;
        let used: Vec<&str> = children(&node, "Relationship").iter().filter_map(|r| r.attr_exact("Id")).collect();
        let mut id = "support-fins".to_string();
        while used.contains(&id.as_str()) {
            id.push_str("-1");
        }
        let edit = insert(&node, &format!("<Relationship Id=\"{id}\" Type=\"{REL}\" Target=\"/{SUPPORTS}\"/>"));
        set_change(&mut changes, name, Some(ZipData::Text(edit_xml(&text, vec![edit])?)));
    }
    let types_text = zip.text("[Content_Types].xml")?;
    let types = xml_tree(&types_text)?;
    let mut type_edits: Vec<Edit> = Vec::new();
    if !children(&types, "Default").iter().any(|n| n.attr_exact("Extension") == Some("json")) {
        type_edits.push(insert(&types, "<Default Extension=\"json\" ContentType=\"application/json\"/>"));
    }
    for node in children(&types, "Override") {
        let part = node.attr_exact("PartName").map(|p| p.strip_prefix('/').unwrap_or(p));
        if part.is_some_and(|p| stale.iter().any(|s| s == p)) {
            type_edits.push((node.start, node.end, String::new()));
        }
    }
    if !type_edits.is_empty() {
        set_change(&mut changes, "[Content_Types].xml", Some(ZipData::Text(edit_xml(&types_text, type_edits)?)));
    }
    // Remove relationships to discarded preview/toolpath members.
    for name in zip.entries.keys() {
        if !name.ends_with(".rels") {
            continue;
        }
        let text = match changes.iter().find(|(n, _)| n == name) {
            Some((_, Some(ZipData::Text(t)))) => t.clone(),
            _ => zip.text(name)?,
        };
        let node = xml_tree(&text)?;
        let base = if name.contains("/_rels/") {
            &name[..name.rfind("/_rels/").unwrap() + 1]
        } else {
            ""
        };
        let edits: Vec<Edit> = children(&node, "Relationship")
            .into_iter()
            .filter(|n| {
                let target = n.attr_exact("Target").unwrap_or("");
                let resolved = match target.strip_prefix('/') {
                    Some(t) => t.to_string(),
                    None => format!("{base}{target}"),
                };
                stale.iter().any(|s| *s == resolved)
            })
            .map(|n| (n.start, n.end, String::new()))
            .collect();
        if !edits.is_empty() {
            set_change(&mut changes, name, Some(ZipData::Text(edit_xml(&text, edits)?)));
        }
    }
    let bytes = zip.patch(&changes)?;
    io.write_project(input, &output, &bytes, args.overwrite)?;
    report.output_written = true;
    if let Some(c) = &cache {
        c.finish(&warn);
    }
    warn("project preserved; slice it again in Bambu Studio before printing.");
    let with_tris = results.iter().filter(|r| !matches!(r.get("triangles"), Some(Json::Num(0.0)))).count();
    let text = format!(
        "{} objects processed, {with_tris} support parts added; re-slice in Bambu Studio",
        results.len()
    );
    Ok(FileResult {
        input: input.to_string(),
        output: Some(output.clone()),
        report: text.clone(),
        json: Json::Obj(vec![
            ("input".into(), Json::Str(input.to_string())),
            ("output".into(), Json::Str(output)),
            ("objects".into(), Json::Arr(results)),
            ("report".into(), Json::Str(text)),
        ]),
        incompatible: false,
    })
}

/// Used by the report formatter for data not needed here.
#[allow(dead_code)]
fn _unused(_: &ReportData) {}
#[allow(dead_code)]
fn _unused2() -> Result<String, String> {
    runtime_identity()
}
