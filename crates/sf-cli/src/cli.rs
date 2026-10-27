//! Port of `plugins/cli/cli.js`: Support Fins from a terminal -- the command line's logic, with no
//! file system of its own. `main.rs` hands it the arguments and a small I/O object, and the tests
//! hand it an in-memory one.
//!
//! Same engine as every plugin: the part is posed here (--rot), then `entry::compute_fins` places
//! the fins exactly as it does for Orca, Cura or Fusion. Flags come from options.json, so a flag is
//! a site setting is a plugin dialog control.

use sf_engine::jsnum;
use sf_io::json::Json;
use sf_io::obj::read_obj;
use sf_io::stl::{read_stl, write_binary_stl};
use sf_io::threemf::{ExtraPart, PartKind, ThreeMfOpts, read_three_mf, write_three_mf};
use sf_io::zip::unzip;

use crate::entry::{
    EngineOptions, compute_fins, nozzle_choice, option_visible, options_from_dialog, report_line,
};
use crate::options::{OptType, OptionDef, flag_of, options};
use crate::{VERSION, js_trim};

/// The file system and the two output streams (`io` of cli.js).
pub trait Io {
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String>;
    fn out(&self, line: &str);
    fn err(&self, line: &str);

    // --- project mode (support-fins.js:14-16): present only where the host has the file system
    // for it (`io.checkProjectOutput`, `io.writeProject`, `io.openProjectCache`)

    /// Whether `check_project_output` and `write_project` exist.
    fn has_project_io(&self) -> bool {
        false
    }
    /// `io.checkProjectOutput(input, output, overwrite)`
    fn check_project_output(
        &self,
        _input: &str,
        _output: &str,
        _overwrite: bool,
    ) -> Result<(), String> {
        Err("project output requires an atomic file-system writer".to_string())
    }
    /// `io.writeProject(input, output, bytes, overwrite)`
    fn write_project(
        &self,
        _input: &str,
        _output: &str,
        _bytes: &[u8],
        _overwrite: bool,
    ) -> Result<(), String> {
        Err("project output requires an atomic file-system writer".to_string())
    }
    /// Whether `open_project_cache` exists.
    fn has_project_cache(&self) -> bool {
        false
    }
    /// `io.openProjectCache({ input, bytes, selection, settings, cacheDir, warn })`
    fn open_project_cache(
        &self,
        _req: &crate::project_cache::CacheRequest,
        _warn: &dyn Fn(&str),
    ) -> Result<Box<dyn crate::project_cache::CacheStore>, String> {
        Err("--resume requires a file-system checkpoint store".to_string())
    }
}

/// `shown` (cli.js:29): a percent option is typed as the site's slider shows it (0-100), not the
/// entry's 0-1.
pub fn shown(o: &OptionDef, v: &Json) -> String {
    match v {
        Json::Num(n) if o.percent => {
            jsnum::to_js_string(jsnum::string_to_number(&jsnum::to_fixed(n * 100.0, 6)))
        }
        other => other.to_js_string(),
    }
}

fn num_text(n: Option<f64>) -> String {
    jsnum::to_js_string(n.unwrap_or(f64::NAN))
}

/// `helpText()` (cli.js:31-78).
pub fn help_text() -> String {
    let mut lines: Vec<String> = [
        "support-fins -- bake breakaway support fins into a 3D print (printfins.com, from a terminal)",
        "",
        "usage: support-fins <part.stl|part.3mf|part.obj>... [options]",
        "",
        "  -o, --output <file>   where to write (.3mf or .stl); one input only.",
        "                        default: <part>-fins.3mf next to the input",
        "  --rot <x,y,z>         pose the part first, in degrees: the site's \"X · Y · Z\" readout",
        "  --fins-only           write only the fins + bed pad, lined up with the part as it is in",
        "                        your file (so not with --rot)",
        "  --object <n|name>     which object of a multi-object 3MF or OBJ to fin (1-based)",
        "  --json                one JSON line per file instead of the summary",
        "  --preserve-project    keep a Bambu Studio/MakerWorld project; add fins to all",
        "                        printable objects on their original plates",
        "  --plate <id>          project mode: select a plate ID from the listing; repeat for several",
        "  --list-objects        project mode: plate summary + build-item numbers; write nothing",
        "  --list-plates         project mode: plate IDs and object counts; write nothing",
        "  --check               with --preserve-project: check settings and meshes; write nothing",
        "                        exit 1 if any selected object is incompatible",
        "  --fin-extruder <n>    project mode: existing filament slot for fins (1-based)",
        "  --disable-native-supports  project mode: disable slicer supports on selected objects",
        "  --overwrite           project mode: allow replacing an existing output",
        "                        default: <part>-fins-project.3mf; repeat --object to select several",
        "                        nozzle: read from project unless --nozzle overrides fin generation",
        "  --resume              project mode: save/reuse completed-object checkpoints",
        "  --cache-dir <path>    with --resume: cache folder (default: <input>.fins-cache)",
        "  -h, --help            this text",
        "  --version             which build this is",
        "",
        "settings (the site's, same defaults):",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for o in options() {
        let flag = flag_of(&o.key);
        let mut arg = String::new();
        if o.typ == OptType::Choice {
            arg = format!(
                " <{}>",
                o.choices
                    .iter()
                    .map(Json::to_js_string)
                    .collect::<Vec<_>>()
                    .join("|")
            );
        } else if o.typ == OptType::Number {
            arg = if o.percent {
                " <0-100>".to_string()
            } else {
                format!(" <{}-{}>", num_text(o.min), num_text(o.max))
            };
        }
        let name = if o.typ == OptType::Bool {
            if matches!(o.default, Json::Bool(true)) {
                format!("--no-{flag}")
            } else {
                format!("--{flag}")
            }
        } else {
            format!("--{flag}{arg}")
        };
        // Units only ("mm", "%"): the site's other hints ("light ⟶ firm") are slider ends.
        let unit = match &o.hint {
            Some(h) if h.starts_with("mm") || h.starts_with('°') || h.starts_with('%') => {
                format!(" {h}")
            }
            _ => String::new(),
        };
        let what = if o.typ == OptType::Bool {
            format!(
                "{} {}{}",
                if matches!(o.default, Json::Bool(true)) {
                    "turn off"
                } else {
                    "turn on"
                },
                o.label.to_lowercase(),
                o.hint.as_ref().map_or(String::new(), |h| format!(" ({h})"))
            )
        } else {
            format!("{}{unit}, default {}", o.label, shown(o, &o.default))
        };
        // `name.padEnd(38)`: by UTF-16 length
        let units = name.encode_utf16().count();
        let pad = 38usize.saturating_sub(units);
        lines.push(format!("  {name}{} {what}", " ".repeat(pad)));
    }
    lines.push(String::new());
    lines.push(
        "Overhangs a fin can't reach are reported, not errors: exit 0. Exit 1 = a file".to_string(),
    );
    lines.push(
        "failed; exit 2 = bad arguments. `--` ends the options (for a file named -x.stl)."
            .to_string(),
    );
    lines.join("\n")
}

/// What `parseArgs` throws: a `UsageError` (said as it is), or another `Error` (`nozzleChoice`'s,
/// which `run` words in the flag's units).
#[derive(Debug)]
pub enum ArgError {
    Usage(String),
    Other(String),
}

/// `UsageError(message)`.
#[allow(non_snake_case)]
fn UsageError(m: impl Into<String>) -> ArgError {
    ArgError::Usage(m.into())
}

/// The parsed command line (`parseArgs`' result).
#[derive(Clone, Debug, Default)]
pub struct Args {
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub rot: Option<[f64; 3]>,
    pub fins_only: bool,
    pub object: Option<String>,
    pub objects: Vec<String>,
    pub json: bool,
    pub help: bool,
    pub version: bool,
    /// `dialog`: `{options.json key: value}` in the entry's units, in the order given.
    pub dialog: Vec<(String, Json)>,
    pub preserve_project: bool,
    pub list_objects: bool,
    pub check: bool,
    pub plates: Vec<String>,
    pub list_plates: bool,
    pub fin_extruder: Option<f64>,
    pub disable_native_supports: bool,
    pub overwrite: bool,
    pub resume: bool,
    pub cache_dir: Option<String>,
}

impl Args {
    fn set_dialog(&mut self, key: &str, v: Json) {
        match self.dialog.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = v,
            None => self.dialog.push((key.to_string(), v)),
        }
    }

    /// `args.dialog[key]`
    pub fn dialog_get(&self, key: &str) -> Option<&Json> {
        self.dialog.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// Port of `parseArgs` (cli.js:83-153): argv (without the program name).
pub fn parse_args(argv: &[String]) -> Result<Args, ArgError> {
    let mut out = Args::default();
    let mut files = false;
    let mut i = 0;
    while i < argv.len() {
        let mut a = argv[i].clone();
        if files {
            out.inputs.push(a);
            i += 1;
            continue;
        }
        if a == "--" {
            files = true;
            i += 1;
            continue;
        }
        let mut value: Option<String> = None;
        let eq = if a.starts_with("--") {
            a.find('=')
        } else {
            None
        };
        if let Some(eq) = eq.filter(|&e| e > 0) {
            value = Some(a[eq + 1..].to_string());
            a = a[..eq].to_string();
        }
        // `next()`: the `=value`, or the next argument
        let next = |i: &mut usize| -> Result<String, ArgError> {
            if let Some(v) = &value {
                return Ok(v.clone());
            }
            if *i + 1 >= argv.len() {
                return Err(UsageError(format!("{a} needs a value")));
            }
            *i += 1;
            Ok(argv[*i].clone())
        };
        if a == "-h" || a == "--help" {
            out.help = true;
        } else if a == "--version" {
            out.version = true;
        } else if a == "-o" || a == "--output" {
            out.output = Some(next(&mut i)?);
        } else if a == "--rot" {
            let v = next(&mut i)?;
            out.rot = Some(parse_rot(&v)?);
        } else if a == "--fins-only" {
            out.fins_only = true;
        } else if a == "--object" {
            let v = next(&mut i)?;
            out.object = Some(v.clone());
            out.objects.push(v);
        } else if a == "--plate" {
            let v = next(&mut i)?;
            if js_trim(&v).is_empty() || v.starts_with("--") {
                return Err(UsageError("--plate needs a plate ID (see --list-plates)"));
            }
            out.plates.push(v);
        } else if a == "--json" {
            out.json = true;
        } else if a == "--cache-dir" {
            let v = next(&mut i)?;
            if js_trim(&v).is_empty() || v.starts_with("--") {
                return Err(UsageError("--cache-dir needs a directory path"));
            }
            out.cache_dir = Some(v);
        } else if a == "--fin-extruder" {
            let v = next(&mut i)?;
            // /^[1-9]\d*$/ and Number.isSafeInteger
            let digits =
                !v.is_empty() && v.as_bytes()[0] != b'0' && v.bytes().all(|b| b.is_ascii_digit());
            let n = jsnum::string_to_number(&v);
            if !digits || n > 9007199254740991.0 {
                return Err(UsageError("--fin-extruder must be a positive integer"));
            }
            out.fin_extruder = Some(n);
        } else if [
            "--preserve-project",
            "--list-objects",
            "--list-plates",
            "--check",
            "--disable-native-supports",
            "--overwrite",
            "--resume",
        ]
        .contains(&a.as_str())
        {
            if value.is_some() {
                return Err(UsageError(format!("{a} takes no value")));
            }
            match a.as_str() {
                "--preserve-project" => out.preserve_project = true,
                "--list-objects" => out.list_objects = true,
                "--check" => out.check = true,
                "--list-plates" => out.list_plates = true,
                "--disable-native-supports" => out.disable_native_supports = true,
                "--overwrite" => out.overwrite = true,
                _ => out.resume = true,
            }
        } else if a.starts_with("--") {
            let neg = a.starts_with("--no-");
            let flag = &a[if neg { 5 } else { 2 }..];
            let Some(o) = options().iter().find(|o| flag_of(&o.key) == flag) else {
                return Err(UsageError(format!("unknown option {a} (see --help)")));
            };
            if o.typ == OptType::Bool {
                if value.is_some() {
                    return Err(UsageError(format!("{a} takes no value")));
                }
                out.set_dialog(&o.key, Json::Bool(!neg));
            } else {
                if neg {
                    return Err(UsageError(format!("unknown option {a} (see --help)")));
                }
                let v = next(&mut i)?;
                if o.typ == OptType::Number {
                    let n = jsnum::string_to_number(&v);
                    if js_trim(&v).is_empty() || !n.is_finite() {
                        return Err(UsageError(format!(
                            "--{flag} must be a number, got {}",
                            Json::Str(v).stringify()
                        )));
                    }
                    out.set_dialog(&o.key, Json::Num(if o.percent { n / 100.0 } else { n }));
                } else if o.key == "nozzle" {
                    let c = nozzle_choice(&Json::Str(v)).map_err(ArgError::Other)?;
                    out.set_dialog(&o.key, c);
                } else {
                    out.set_dialog(&o.key, Json::Str(v));
                }
            }
        } else if a.starts_with('-') && a != "-" {
            return Err(UsageError(format!("unknown option {a} (see --help)")));
        } else {
            out.inputs.push(a);
        }
        i += 1;
    }
    Ok(out)
}

/// `parseRot` (cli.js:155-161).
fn parse_rot(s: &str) -> Result<[f64; 3], ArgError> {
    let v: Vec<f64> = s
        .split(',')
        .map(|t| jsnum::string_to_number(js_trim(t)))
        .collect();
    if v.len() != 3 || v.iter().any(|n| !n.is_finite()) {
        return Err(UsageError(format!(
            "--rot takes three angles in degrees, x,y,z -- got {}",
            Json::Str(s.to_string()).stringify()
        )));
    }
    Ok([v[0], v[1], v[2]])
}

/// Port of `rotationMatrix` (cli.js:170-180): column-major 3x3 (three.js Matrix3.elements) for the
/// site's rotation readout: "X a · Y b · Z c" is three.js Euler(a, b, c, 'XYZ'), i.e. R = Rx * Ry
/// * Rz, so the numbers on printfins.com pose the part the same way here.
pub fn rotation_matrix(rot: [f64; 3]) -> [f64; 9] {
    let r = |d: f64| (d * std::f64::consts::PI) / 180.0;
    let (cx, sx) = (jsnum::cos(r(rot[0])), jsnum::sin(r(rot[0])));
    let (cy, sy) = (jsnum::cos(r(rot[1])), jsnum::sin(r(rot[1])));
    let (cz, sz) = (jsnum::cos(r(rot[2])), jsnum::sin(r(rot[2])));
    // three.js Matrix4.makeRotationFromEuler, order 'XYZ', stored by column
    [
        cy * cz,
        cx * sz + sx * sy * cz,
        sx * sz - cx * sy * cz,
        -cy * sz,
        cx * cz - sx * sy * sz,
        sx * cz + cx * sy * sz,
        sy,
        -sx * cy,
        cx * cy,
    ]
}

/// Port of `pose` (cli.js:183-194): pose a soup in float64 (Orca poses in double; float32 can
/// flip a borderline tine).
pub fn pose(pos: &[f64], rot: Option<[f64; 3]>) -> Vec<f64> {
    let Some(rot) = rot else { return pos.to_vec() };
    let m = rotation_matrix(rot);
    let mut out = vec![0.0; pos.len()];
    let mut i = 0;
    while i < pos.len() {
        let (x, y, z) = (pos[i], pos[i + 1], pos[i + 2]);
        out[i] = m[0] * x + m[3] * y + m[6] * z;
        out[i + 1] = m[1] * x + m[4] * y + m[7] * z;
        out[i + 2] = m[2] * x + m[5] * y + m[8] * z;
        i += 3;
    }
    out
}

/// `triples` (cli.js:196-202).
fn triples(flat: &[f64], add: [f64; 3]) -> Vec<[f64; 3]> {
    flat.chunks(3)
        .map(|c| [c[0] + add[0], c[1] + add[1], c[2] + add[2]])
        .collect()
}

/// One object of a multi-object file: `{ name, positions, extras? }`.
struct Obj {
    name: String,
    positions: Vec<f64>,
    extras: Vec<sf_io::threemf::ReadExtra>,
}

/// Port of `pickObject` (cli.js:207-216): --object out of a multi-object file: a 1-based index or
/// a name.
fn pick_object(objs: &[Obj], object: Option<&str>) -> Result<usize, String> {
    let Some(object) = object else {
        return Err(format!(
            "has {} objects ({}); pick one with --object",
            objs.len(),
            objs.iter()
                .enumerate()
                .map(|(i, o)| format!("{} {}", i + 1, o.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let by_index = if !object.is_empty() && object.bytes().all(|b| b.is_ascii_digit()) {
        let n = jsnum::string_to_number(object);
        // objs[Number(object) - 1]: an index past either end is undefined
        if n >= 1.0 && n <= objs.len() as f64 {
            Some(n as usize - 1)
        } else {
            None
        }
    } else {
        None
    };
    by_index
        .or_else(|| objs.iter().position(|o| o.name == object))
        .ok_or_else(|| {
            format!(
                "no object {}; it has {}",
                Json::Str(object.to_string()).stringify(),
                objs.iter()
                    .map(|o| o.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// What `readPart` returns: `{ positions, slicer? }`.
struct Read {
    positions: Vec<f64>,
    /// `slicer`: the uncut part and the slicer's parts beside it.
    slicer: Option<(Vec<f64>, Vec<sf_io::threemf::ReadExtra>)>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Port of `readPart` (cli.js:218-250).
fn read_part(
    path: &str,
    bytes: &[u8],
    object: Option<&str>,
    warn: &dyn Fn(&str),
) -> Result<Read, String> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".3mf") {
        // Our own export holds part + fins as one object: finning it would put fins under the old
        // fins. Refuse rather than print that.
        let parts = unzip(bytes)?;
        if let Some(model) = parts.get("3D/3dmodel.model")
            && String::from_utf8_lossy(model)
                .contains("<metadata name=\"Application\">Support Fins</metadata>")
        {
            return Err(
                "already has fins (a Support Fins export): fin the original part instead".into(),
            );
        }
        let m = read_three_mf(bytes)?;
        let objs: Vec<Obj> = m
            .objects
            .iter()
            .map(|o| Obj {
                name: o.name.clone(),
                positions: o.positions.clone(),
                extras: o.extras.clone(),
            })
            .collect();
        // a slicer's negative parts are cut out, as on the site, and the fins built for the cut
        // part; a 3MF output carries them and the modifiers and support blockers back out beside
        // the part as the file had it, as the site does (#104)
        let prepared = |o: &Obj| -> Read {
            let cut = crate::cut::cut_negatives(&o.positions, &o.extras);
            let (positions, uncut, why) = (cut.positions, cut.uncut, cut.why);
            let why_text = match why {
                Some("open") => {
                    Some("the part is not a closed solid, so its fins are for the uncut shape")
                }
                Some("whole") => Some("they would remove the whole part"),
                Some("load") => Some("the cutter did not load"),
                _ => None,
            };
            if let Some(w) = why_text {
                warn(&format!("negative parts not cut: {w}"));
            } else if uncut != 0 {
                warn(&format!(
                    "{} not cut: not a closed solid",
                    plural(uncut, "negative part", "negative parts")
                ));
            }
            Read {
                positions,
                slicer: if o.extras.is_empty() {
                    None
                } else {
                    Some((o.positions.clone(), o.extras.clone()))
                },
            }
        };
        if objs.len() <= 1 && object.is_none() {
            return Ok(match objs.first() {
                Some(o) => prepared(o),
                None => Read {
                    positions: m.positions,
                    slicer: None,
                },
            });
        }
        let i = pick_object(&objs, object)?;
        return Ok(prepared(&objs[i]));
    }
    if lower.ends_with(".stl") {
        return Ok(Read {
            positions: read_stl(bytes).map_err(|e| e.0)?,
            slicer: None,
        });
    }
    if lower.ends_with(".obj") {
        // OBJ has no unit (read as mm); several `o` objects pick like a 3MF plate's
        let objs: Vec<Obj> = read_obj(bytes)?
            .objects
            .into_iter()
            .map(|o| Obj {
                name: o.name,
                positions: o.positions,
                extras: Vec::new(),
            })
            .collect();
        let i = if objs.len() == 1 && object.is_none() {
            0
        } else {
            pick_object(&objs, object)?
        };
        return Ok(Read {
            positions: objs[i].positions.clone(),
            slicer: None,
        });
    }
    Err("reads .stl, .3mf and .obj only".into())
}

fn strip_ext(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    for e in [".stl", ".3mf", ".obj"] {
        if lower.ends_with(e) {
            return s[..s.len() - e.len()].to_string();
        }
    }
    s.to_string()
}

/// `a.replace(/^\.[\\/]/, '')`
fn trim_dot_slash(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'.' && (b[1] == b'/' || b[1] == b'\\') {
        &s[2..]
    } else {
        s
    }
}

/// `defaultOutput` (cli.js:254-257).
fn default_output(input: &str, fins_only: bool) -> String {
    let base = strip_ext(input);
    if fins_only {
        format!("{base}-fins-only.stl")
    } else {
        format!("{base}-fins.3mf")
    }
}

/// What `finFile` returns: `{ input, output, stats, report }`, or a project mode's
/// `{ input, output, objects, report, ... }`. `json` is the object `JSON.stringify` writes.
#[derive(Clone, Debug)]
pub struct FileResult {
    pub input: String,
    pub output: Option<String>,
    pub report: String,
    pub json: Json,
    /// `r.summary.incompatible` (a `--check` that found an incompatible object)
    pub incompatible: bool,
}

impl FileResult {
    /// `JSON.stringify(r)`
    pub fn to_json(&self) -> String {
        self.json.stringify()
    }
}

/// What `finFile` throws: the message, and a project run's report (`e.projectReport`).
#[derive(Clone, Debug)]
pub struct FinError {
    pub message: String,
    pub report: Option<crate::project_report::ReportData>,
}

impl From<&str> for FinError {
    fn from(message: &str) -> FinError {
        FinError {
            message: message.to_string(),
            report: None,
        }
    }
}

impl From<String> for FinError {
    fn from(message: String) -> FinError {
        FinError {
            message,
            report: None,
        }
    }
}

/// Port of `finFile` (cli.js:263-293): fin one file. The part is written as the engine sees it:
/// posed, centred over the plate origin, on z = 0 -- the site's export frame -- with the fins in
/// that frame.
pub fn fin_file(
    input: &str,
    args: &Args,
    io: &dyn Io,
    engine: &EngineOptions,
) -> Result<FileResult, FinError> {
    if args.preserve_project {
        return crate::project::fin_project(input, args, io);
    }
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| default_output(input, args.fins_only));
    let out_lower = output.to_ascii_lowercase();
    if !(out_lower.ends_with(".3mf") || out_lower.ends_with(".stl")) {
        return Err(format!("output {output}: write .3mf or .stl").into());
    }
    if args.fins_only && !out_lower.ends_with(".stl") {
        return Err("--fins-only writes an .stl".into());
    }
    if trim_dot_slash(&output) == trim_dot_slash(input) {
        return Err("won't overwrite the input; pick another -o".into());
    }
    let warn_line = |line: &str| io.err(&format!("support-fins: {input}: warning: {line}"));
    let read = read_part(input, &io.read(input)?, args.object.as_deref(), &warn_line)?;
    let posed = pose(&read.positions, args.rot);
    let r = compute_fins(&posed, engine)?;
    let base = input.rsplit(['/', '\\']).next().unwrap_or("");
    let name = if base.is_empty() {
        "part".to_string()
    } else {
        strip_ext(base)
    };
    if args.fins_only && r.triangles.is_empty() {
        let report = format!("{}; no fins, nothing written", report_line(&r.stats));
        return Ok(file_result(input, None, report, &r.stats));
    }
    // The fins come back in the engine's seated frame. --fins-only maps them back onto the part as
    // it is in the file (fin - offset); otherwise the part moves to them.
    let tri64: Vec<f64> = r.triangles.iter().map(|&v| v as f64).collect();
    let off = r.offset;
    let fins = triples(
        &tri64,
        if args.fins_only {
            [-off.x, -off.y, -off.z]
        } else {
            [0.0, 0.0, 0.0]
        },
    );
    let at = [off.x, off.y, off.z];
    // an STL is the cut part: the modifiers and blockers can't go in it -- said, not dropped
    // quietly
    let rest = read.slicer.as_ref().map_or(0, |(_, ex)| {
        ex.iter().filter(|e| e.kind != PartKind::Negative).count()
    });
    if rest != 0 && !out_lower.ends_with(".3mf") {
        warn_line(&format!(
            "left out {rest} {}: an STL can't carry them, write a .3mf",
            if rest == 1 {
                "modifier/support blocker"
            } else {
                "modifiers/support blockers"
            }
        ));
    }
    let part = triples(&posed, at);
    let blob: Vec<u8> = if args.fins_only {
        write_binary_stl(&fins, &format!("{name} fins"))
    } else if out_lower.ends_with(".3mf") {
        three_mf(&part, &fins, &name, read.slicer.as_ref(), args.rot, at)
    } else {
        let mut both = part;
        both.extend(fins);
        write_binary_stl(&both, &name)
    };
    io.write(&output, &blob)?;
    let report = report_line(&r.stats);
    Ok(file_result(input, Some(output), report, &r.stats))
}

/// The `{ input, output, stats, report }` of a plain run.
fn file_result(
    input: &str,
    output: Option<String>,
    report: String,
    stats: &crate::entry::Stats,
) -> FileResult {
    FileResult {
        input: input.to_string(),
        json: Json::Obj(vec![
            ("input".into(), Json::Str(input.to_string())),
            (
                "output".into(),
                output.clone().map_or(Json::Null, Json::Str),
            ),
            ("stats".into(), stats.to_json()),
            ("report".into(), Json::Str(report.clone())),
        ]),
        output,
        report,
        incompatible: false,
    }
}

/// Port of `threeMF` (cli.js:301-306): the 3MF output. A file with a slicer's own parts goes out
/// as the file had it: the part uncut beside its negative parts, modifiers and blockers, all
/// posed the same way, for the slicer to cut again -- in the SEPARATE form, since a negative part
/// cuts every part of its object and the walls in its hole are among them.
fn three_mf(
    part: &[[f64; 3]],
    fins: &[[f64; 3]],
    name: &str,
    slicer: Option<&(Vec<f64>, Vec<sf_io::threemf::ReadExtra>)>,
    rot: Option<[f64; 3]>,
    at: [f64; 3],
) -> Vec<u8> {
    let Some((positions, extras)) = slicer else {
        return write_three_mf(part, fins, name, &ThreeMfOpts::default());
    };
    let placed = |flat: &[f64]| triples(&pose(flat, rot), at);
    write_three_mf(
        &placed(positions),
        fins,
        name,
        &ThreeMfOpts {
            separate: true,
            extras: extras
                .iter()
                .map(|e| ExtraPart {
                    kind: e.kind,
                    name: e.name.clone(),
                    settings: e.settings.clone(),
                    tris: placed(&e.positions),
                })
                .collect(),
            ..Default::default()
        },
    )
}

/// `flagMessage` (cli.js:370-381): "coverage must be 0..1 (a percent control's value / 100), got
/// 1.5" -> "--coverage must be 0..100, got 150".
fn flag_message(msg: &str) -> String {
    // /^(\S+) must be (\S+?)\.\.(\S+?)(?: \(a percent[^)]*\))?, got (.*)$/
    fn parse(msg: &str) -> Option<(&str, &str, &str, &str)> {
        let (key, rest) = msg.split_once(' ')?;
        let rest = rest.strip_prefix("must be ")?;
        let (range, got) = rest.split_once(", got ")?;
        let range = match range.find(" (a percent") {
            Some(p) if range.ends_with(')') && !range[p..range.len() - 1].contains(')') => {
                &range[..p]
            }
            _ => range,
        };
        let (lo, hi) = range.split_once("..")?;
        if key.is_empty() || lo.is_empty() || hi.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        Some((key, lo, hi, got))
    }
    let o = parse(msg).and_then(|m| options().iter().find(|x| x.key == m.0).map(|o| (m, o)));
    let Some(((_, _, _, got), o)) = o else {
        let first = msg
            .split(' ')
            .next()
            .filter(|k| !k.is_empty() && msg.contains(' '));
        let opt = first.and_then(|k| options().iter().find(|x| x.key == k));
        return match (first, opt) {
            (Some(k), Some(opt)) => format!("--{}{}", flag_of(&opt.key), &msg[k.len()..]),
            _ => msg.to_string(),
        };
    };
    let g = jsnum::string_to_number(got);
    let show = |v: f64| shown(o, &Json::Num(v));
    format!(
        "--{} must be {}..{}, got {}",
        flag_of(&o.key),
        show(o.min.unwrap_or(f64::NAN)),
        show(o.max.unwrap_or(f64::NAN)),
        if g.is_finite() {
            show(g)
        } else {
            got.to_string()
        }
    )
}

/// Port of `run` (cli.js:316-368): `argv` are the arguments after the program name. Returns the
/// exit code: 0 ok, 1 a file failed, 2 bad arguments.
pub fn run(argv: &[String], io: &dyn Io) -> i32 {
    let usage = |e: String| -> i32 {
        io.err(&format!("support-fins: {e}"));
        2
    };
    let args = match parse_args(argv) {
        Ok(a) => a,
        Err(ArgError::Usage(m)) => return usage(m),
        // optionsFromDialog speaks in the entry's units; say it in the flag's.
        Err(ArgError::Other(m)) => return usage(flag_message(&m)),
    };
    let checked = (|| -> Result<EngineOptions, String> {
        if args.help {
            return Err("\0help".into());
        }
        if args.version {
            return Err("\0version".into());
        }
        if args.inputs.is_empty() {
            return Err("no input file (see --help)".into());
        }
        let u = |s: &str| Err(s.to_string());
        if args.output.as_deref().is_some_and(|o| !o.is_empty()) && args.inputs.len() > 1 {
            return u("-o takes one input; with several, each is written next to its input");
        }
        if args.preserve_project && (args.rot.is_some() || args.fins_only) {
            return u(
                "--preserve-project keeps the existing pose and writes a full project; do not combine with --rot or --fins-only",
            );
        }
        if !args.preserve_project
            && (args.check
                || args.list_objects
                || args.list_plates
                || !args.plates.is_empty()
                || args.fin_extruder.is_some()
                || args.disable_native_supports
                || args.overwrite)
        {
            return u(
                "--plate, --check, --list-objects, --list-plates, --fin-extruder, --disable-native-supports and --overwrite require --preserve-project",
            );
        }
        if args.check
            && (args.list_objects || args.list_plates || args.output.is_some() || args.overwrite)
        {
            return u(
                "--check writes nothing; do not combine with --list-objects, --list-plates, --output or --overwrite",
            );
        }
        if args.list_objects && args.list_plates {
            return u("use --list-objects or --list-plates, not both");
        }
        if !args.preserve_project && args.objects.len() > 1 {
            return u("repeat --object only with --preserve-project");
        }
        if args.resume && !args.preserve_project {
            return u("--resume requires --preserve-project");
        }
        if args.cache_dir.is_some() && !args.resume {
            return u("--cache-dir requires --resume");
        }
        for (on, flag) in [
            (args.list_objects, "--list-objects"),
            (args.list_plates, "--list-plates"),
            (args.check, "--check"),
        ] {
            if on && args.resume {
                return Err(format!(
                    "{flag} does not generate fins; do not combine with --resume"
                ));
            }
        }
        if args.fins_only && args.rot.is_some() {
            return u(
                "--fins-only lines up with the part as it is in your file, so it can't take --rot: pose the part in your file, or write part + fins (.3mf)",
            );
        }
        // optionsFromDialog speaks in the entry's units; say it in the flag's.
        let e = options_from_dialog(&args.dialog).map_err(|m| format!("\0flag{m}"))?;
        // No setting the math ignores goes by silently (the site hides these controls).
        for (key, _) in &args.dialog {
            if !option_visible(key, &args.dialog)? {
                io.err(&format!(
                    "support-fins: warning: --{} does nothing with these settings",
                    flag_of(key)
                ));
            }
        }
        Ok(e)
    })();
    let engine = match checked {
        Ok(e) => e,
        Err(m) if m == "\0help" => {
            io.out(&help_text());
            return 0;
        }
        Err(m) if m == "\0version" => {
            io.out(&format!("support-fins {VERSION}"));
            return 0;
        }
        Err(m) => {
            return usage(match m.strip_prefix("\0flag") {
                Some(inner) => flag_message(inner),
                None => m,
            });
        }
    };
    let mut failed = 0;
    for input in &args.inputs {
        match fin_file(input, &args, io, &engine) {
            Ok(r) => {
                io.out(&if args.json {
                    r.to_json()
                } else {
                    format!(
                        "{} -> {}: {}",
                        r.input,
                        r.output.as_deref().unwrap_or("(nothing)"),
                        r.report
                    )
                });
                if args.check && r.incompatible {
                    failed += 1;
                }
            }
            Err(e) => {
                failed += 1;
                io.err(&if args.json {
                    let mut f = vec![
                        ("input".to_string(), Json::Str(input.clone())),
                        ("error".to_string(), Json::Str(e.message.clone())),
                    ];
                    if let Some(r) = &e.report {
                        f.extend(r.to_json_fields());
                    }
                    Json::Obj(f).stringify()
                } else {
                    format!(
                        "support-fins: {input}: {}{}",
                        e.message,
                        e.report.as_ref().map_or(String::new(), |r| format!(
                            "\n{}",
                            crate::project_report::format_project_report(r)
                        ))
                    )
                });
            }
        }
    }
    if failed != 0 { 1 } else { 0 }
}
