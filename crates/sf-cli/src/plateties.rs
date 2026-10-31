//! `support-fins <plate.3mf> --plate-ties`: tie neighbouring tall, thin objects of a multi-object 3MF
//! together (issue #50) and write the plate as one merged object.
//!
//! Not in cli.js: new behaviour. `support-fins --help` stays the JS's (cli-parity compares it byte for
//! byte); `support-fins --plate-ties --help` prints this mode's own. Each object keeps its place on the
//! file's plate; by default each is finned as a single part would be (the same settings flags), seated
//! on the bed where it stood; `--no-fins` ties the bare objects, seated on the bed.

use std::time::Instant;

use sf_engine::plateties::{Overrides, Plate, PlateTies, TieModel};
use sf_io::json::Json;
use sf_io::limits::{LoadOptions, check_finite};
use sf_io::plateties::{PLATE_TIES_PART, PlateTiesDoc, settings_from_text};
use sf_io::printer::{default_profile, profile, profiles};
use sf_io::threemf::{Model, read_three_mf_with, write_plate_3mf_parts_checked};

use crate::cli::{Io, parse_args};
use crate::entry::{compute_fins, options_from_dialog, report_line};

/// Is this a plate-ties run (`--plate-ties`, or `--calibration` for the calibration print, anywhere
/// before a `--`)?
pub fn wants(argv: &[String]) -> bool {
    argv.iter()
        .take_while(|a| *a != "--")
        .any(|a| a == "--plate-ties" || a == "--calibration" || a.starts_with("--calibration="))
}

/// This mode's help.
pub fn help_text() -> String {
    let mut lines = vec![
        "support-fins --plate-ties -- tie neighbouring tall, thin objects of a plate together".to_string(),
        String::new(),
        "usage: support-fins <plate.3mf> --plate-ties [options] [settings]".to_string(),
        "       support-fins --calibration plate-ties [-o out.3mf] [--printer id] [--tie-settings json] [settings]"
            .to_string(),
        String::new(),
        "  -o, --output <file>       where to write (.3mf); default: <plate>-ties.3mf next to the input".into(),
        "  --printer <id>            the printer profile the tie defaults come from (default: the first below)".into(),
        "  --tie-settings <json>     plate-tie settings over the profile's, e.g. '{\"maxGap\":6,\"contact\":\"face\"}'".into(),
        "                            keys: maxGap minRatio minHeight contact (edge|face|anywhere) bandWidth".into(),
        "                            tiesPerPair contactThickness middleThickness heightFrom heightTo, and".into(),
        "                            tunables: { depth arch neck sink clearance bedClear facing minOverlap".into(),
        "                            maxRelief reach }".into(),
        "  --no-fins                 tie the bare objects; by default each is finned with the settings flags".into(),
        "  --list-printers           the printer profiles; write nothing".into(),
        "  --calibration plate-ties  write the Plate ties calibration print instead (no input): three pairs".into(),
        "                            of upright 38 x 40 mm coins, each pair tied at another thickness".into(),
        "                            (labelled 1-3, thinnest first, 2 the settings' own), each coin finned".into(),
        "                            with the settings flags; default output plate-ties-coupon.3mf".into(),
        "  --json                    one JSON line instead of the summary".into(),
        "  --allow-large             load past the size limits".into(),
        String::new(),
        "The plate is written as ONE object (every part, fin and tie), so a slicer cannot pull the tied".into(),
        "objects apart. The other settings flags are the ones `support-fins --help` lists.".into(),
        String::new(),
        "printers:".into(),
    ];
    for p in profiles() {
        lines.push(format!(
            "  {}{}  {} ({})",
            p.id,
            if p.is_default { " (default)" } else { "" },
            p.name,
            p.kind.as_str()
        ));
    }
    lines.join("\n")
}

struct Own {
    printer: Option<String>,
    settings: Option<String>,
    no_fins: bool,
    list_printers: bool,
    calibration: Option<String>,
    rest: Vec<String>,
}

fn own_flags(argv: &[String]) -> Result<Own, String> {
    let mut o = Own {
        printer: None,
        settings: None,
        no_fins: false,
        list_printers: false,
        calibration: None,
        rest: Vec::new(),
    };
    let mut i = 0;
    let mut files = false;
    while i < argv.len() {
        let a = &argv[i];
        if files {
            o.rest.push(a.clone());
            i += 1;
            continue;
        }
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            i += 1;
            argv.get(i).cloned().ok_or(format!("{flag} needs {what}"))
        };
        match flag {
            "--" => {
                files = true;
                o.rest.push(a.clone());
            }
            "--plate-ties" => {}
            "--printer" => o.printer = Some(value("a printer id (see --list-printers)")?),
            "--tie-settings" => o.settings = Some(value("JSON settings")?),
            "--no-fins" => o.no_fins = true,
            "--list-printers" => o.list_printers = true,
            "--calibration" => o.calibration = Some(value("a print (plate-ties)")?),
            _ => o.rest.push(a.clone()),
        }
        i += 1;
    }
    Ok(o)
}

/// One object, placed.
struct Placed {
    name: String,
    part: Vec<f64>,
    fins: Vec<f64>,
    report: Option<String>,
}

/// The middle of a soup's box in X and Y, and its lowest Z.
pub fn bbox_centre(p: &[f64]) -> [f64; 3] {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in p.chunks(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]]
}

/// An object finned as a single part would be ([`compute_fins`], the settings flags), seated on the
/// bed and moved back to where it stood: its box's middle over the same spot.
pub struct Finned {
    /// The part, plate frame (flat triangle soup).
    pub part: Vec<f64>,
    /// Its fins and pad, the same frame.
    pub fins: Vec<f64>,
    /// The single-part summary line (`report_line`).
    pub report: String,
}

/// Fin `positions` and keep it where it stood (see [`Finned`]).
pub fn fin_in_place(
    positions: &[f64],
    engine: &crate::entry::EngineOptions,
) -> Result<Finned, String> {
    let at = bbox_centre(positions);
    let r = compute_fins(positions, engine)?;
    // the engine seats the part over the origin; put it back where it stood on the plate
    let seated: Vec<f64> = positions
        .chunks(3)
        .flat_map(|c| [c[0] + r.offset.x, c[1] + r.offset.y, c[2] + r.offset.z])
        .collect();
    let s = bbox_centre(&seated);
    let (dx, dy) = (at[0] - s[0], at[1] - s[1]);
    Ok(Finned {
        part: seated
            .chunks(3)
            .flat_map(|c| [c[0] + dx, c[1] + dy, c[2]])
            .collect(),
        fins: r
            .triangles
            .chunks(3)
            .flat_map(|c| [c[0] as f64 + dx, c[1] as f64 + dy, c[2] as f64])
            .collect(),
        report: report_line(&r.stats),
    })
}

/// Run a plate-ties command line. Exit codes as `cli::run`: 0 ok, 1 the file failed, 2 bad arguments.
pub fn run(argv: &[String], io: &dyn Io) -> i32 {
    let usage = |e: &str| {
        io.err(&format!("support-fins: {e}"));
        2
    };
    let own = match own_flags(argv) {
        Ok(o) => o,
        Err(e) => return usage(&e),
    };
    if own.list_printers {
        for p in profiles() {
            io.out(&format!(
                "{}\t{}\t{}{}",
                p.id,
                p.name,
                p.kind.as_str(),
                if p.is_default { "\tdefault" } else { "" }
            ));
        }
        return 0;
    }
    let args = match parse_args(&own.rest) {
        Ok(a) => a,
        Err(crate::cli::ArgError::Usage(m) | crate::cli::ArgError::Other(m)) => return usage(&m),
    };
    if args.help {
        io.out(&help_text());
        return 0;
    }
    if let Some(print) = &own.calibration {
        if print != "plate-ties" {
            return usage(&format!(
                "no calibration print {print:?} (there is plate-ties)"
            ));
        }
        if !args.inputs.is_empty() || own.no_fins {
            return usage(
                "--calibration plate-ties makes its own plate: no input file, no --no-fins",
            );
        }
    } else if args.inputs.len() != 1 {
        return usage("--plate-ties takes one plate (.3mf)");
    }
    if args.preserve_project || args.fins_only || args.rot.is_some() || args.object.is_some() {
        return usage(
            "--plate-ties ties the whole plate as it stands: not with --preserve-project, --fins-only, --rot or --object",
        );
    }
    let prof = match &own.printer {
        None => default_profile(),
        Some(id) => match profile(id) {
            Some(p) => p,
            None => return usage(&format!("no printer {id:?} (see --list-printers)")),
        },
    };
    let settings = match settings_from_text(own.settings.as_deref().unwrap_or(""), &prof.plate_ties)
    {
        Ok(s) => s,
        Err(e) => return usage(&format!("--tie-settings: {e}")),
    };
    let engine = match options_from_dialog(&args.dialog) {
        Ok(e) => e,
        Err(e) => return usage(&e),
    };
    if own.calibration.is_some() {
        return match calibration_print(&args, prof.id.as_str(), &settings, &engine, io) {
            Ok(line) => {
                io.out(&line);
                0
            }
            Err(e) => {
                io.err(&format!("support-fins: {e}"));
                1
            }
        };
    }
    let input = &args.inputs[0];
    match tie_plate(input, &args, &own, prof.id.as_str(), &settings, &engine, io) {
        Ok(line) => {
            io.out(&line);
            0
        }
        Err(e) => {
            io.err(&format!("support-fins: {input}: {e}"));
            1
        }
    }
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1e4).round() / 10.0
}

fn tie_plate(
    input: &str,
    args: &crate::cli::Args,
    own: &Own,
    profile_id: &str,
    settings: &PlateTies,
    engine: &crate::entry::EngineOptions,
    io: &dyn Io,
) -> Result<String, String> {
    if !input.to_ascii_lowercase().ends_with(".3mf") {
        return Err("--plate-ties reads a multi-object .3mf".into());
    }
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{}-ties.3mf", &input[..input.len() - 4]));
    if !output.to_ascii_lowercase().ends_with(".3mf") {
        return Err(format!("output {output}: --plate-ties writes a .3mf"));
    }
    if output == input {
        return Err("won't overwrite the input; pick another -o".into());
    }
    let load = LoadOptions {
        allow_large: args.allow_large,
    };
    let t_load = Instant::now();
    let m = read_three_mf_with(&io.read(input)?, &load).map_err(String::from)?;
    let load_ms = ms(t_load);
    if m.objects.len() < 2 {
        return Err(format!(
            "has {} object{}: plate ties need two or more",
            m.objects.len(),
            if m.objects.len() == 1 { "" } else { "s" }
        ));
    }
    let warn = |w: &str| io.err(&format!("support-fins: {input}: warning: {w}"));
    let t_fins = Instant::now();
    let mut placed: Vec<Placed> = Vec::with_capacity(m.objects.len());
    let mut left_out = 0;
    for o in &m.objects {
        check_finite(&o.positions, "this file")?;
        // negative parts cut, as a single part is; the merged object carries no slicer parts
        let positions = if o.extras.is_empty() {
            o.positions.clone()
        } else {
            left_out += o
                .extras
                .iter()
                .filter(|e| e.kind != sf_io::threemf::PartKind::Negative)
                .count();
            crate::cut::cut_negatives(&o.positions, &o.extras).positions
        };
        if own.no_fins {
            let at = bbox_centre(&positions);
            let part: Vec<f64> = positions
                .chunks(3)
                .flat_map(|c| [c[0], c[1], c[2] - at[2]])
                .collect();
            placed.push(Placed {
                name: o.name.clone(),
                part,
                fins: vec![],
                report: None,
            });
            continue;
        }
        let f = fin_in_place(&positions, engine).map_err(|e| format!("{}: {e}", o.name))?;
        placed.push(Placed {
            name: o.name.clone(),
            part: f.part,
            fins: f.fins,
            report: Some(f.report),
        });
    }
    let fins_ms = ms(t_fins);
    if left_out != 0 {
        warn(&format!(
            "left out {left_out} modifiers/support blockers: one merged object can't carry them"
        ));
    }
    let models: Vec<TieModel> = placed
        .iter()
        .enumerate()
        .map(|(i, p)| TieModel {
            key: format!("{} {}", i + 1, p.name),
            part: p.part.clone(),
            supports: p.fins.clone(),
        })
        .collect();
    let t_measure = Instant::now();
    let plate = Plate::new(&models)?;
    let measure_ms = ms(t_measure);
    let t_detect = Instant::now();
    let pairs = plate.detect(settings, &Overrides::default())?;
    let detect_ms = ms(t_detect);
    let t_ties = Instant::now();
    let ties = plate.build(settings, &pairs)?;
    let ties_ms = ms(t_ties);
    let three = |v: &[f64]| -> Vec<[f64; 3]> { v.chunks(3).map(|c| [c[0], c[1], c[2]]).collect() };
    let mut fin_tris: Vec<[f64; 3]> = placed.iter().flat_map(|p| three(&p.fins)).collect();
    fin_tris.extend(ties.tris.iter().copied());
    let base = input.rsplit(['/', '\\']).next().unwrap_or("plate");
    let name = base[..base.len() - 4].to_string();
    let model = Model {
        part_tris: placed.iter().flat_map(|p| three(&p.part)).collect(),
        fin_tris,
        name: name.clone(),
        iface: vec![],
        colors: None,
        extras: vec![],
    };
    let doc = PlateTiesDoc {
        profile: profile_id.to_string(),
        settings: settings.clone(),
        overrides: Overrides::default(),
    };
    let t_write = Instant::now();
    let bytes = write_plate_3mf_parts_checked(
        &[model],
        &name,
        false,
        None,
        None,
        &[(PLATE_TIES_PART, doc.to_json().stringify())],
    )?;
    io.write(&output, &bytes)?;
    let write_ms = ms(t_write);
    let placed_ties: usize = ties.pairs.iter().map(|p| p.placed).sum();
    let asked: usize = ties.pairs.iter().map(|p| p.requested).sum();
    let plate = &plate;
    let notes: Vec<String> = ties
        .pairs
        .iter()
        .flat_map(|p| {
            p.notes.iter().map(move |n| {
                format!(
                    "{} - {}: {}",
                    plate.key(p.pair.a),
                    plate.key(p.pair.b),
                    n.text
                )
            })
        })
        .collect();
    if args.json {
        let num = |v: f64| Json::Num(v);
        let pairs_json = Json::Arr(
            ties.pairs
                .iter()
                .map(|p| {
                    Json::Obj(vec![
                        ("a".into(), Json::Str(plate.key(p.pair.a).into())),
                        ("b".into(), Json::Str(plate.key(p.pair.b).into())),
                        ("gap".into(), num(p.pair.gap)),
                        ("placed".into(), num(p.placed as f64)),
                        (
                            "notes".into(),
                            Json::Arr(p.notes.iter().map(|n| Json::Str(n.text.clone())).collect()),
                        ),
                    ])
                })
                .collect(),
        );
        return Ok(Json::Obj(vec![
            ("input".into(), Json::Str(input.into())),
            ("output".into(), Json::Str(output)),
            ("printer".into(), Json::Str(profile_id.into())),
            ("objects".into(), num(placed.len() as f64)),
            ("pairs".into(), pairs_json),
            ("ties".into(), num(placed_ties as f64)),
            ("tieTriangles".into(), num((ties.tris.len() / 3) as f64)),
            (
                "fins".into(),
                Json::Arr(
                    placed
                        .iter()
                        .map(|p| p.report.clone().map_or(Json::Null, Json::Str))
                        .collect(),
                ),
            ),
            (
                "ms".into(),
                Json::Obj(vec![
                    ("load".into(), num(load_ms)),
                    ("fins".into(), num(fins_ms)),
                    ("measure".into(), num(measure_ms)),
                    ("detect".into(), num(detect_ms)),
                    ("ties".into(), num(ties_ms)),
                    ("write".into(), num(write_ms)),
                ]),
            ),
        ])
        .stringify());
    }
    for n in &notes {
        warn(n);
    }
    Ok(format!(
        "{input} -> {output}: {} objects, {} pair{}, {placed_ties} of {asked} ties",
        placed.len(),
        ties.pairs.len(),
        if ties.pairs.len() == 1 { "" } else { "s" },
    ))
}

/// `--calibration plate-ties`: the calibration print (`crate::calibration`), each coin finned with the
/// settings flags.
fn calibration_print(
    args: &crate::cli::Args,
    profile_id: &str,
    settings: &PlateTies,
    engine: &crate::entry::EngineOptions,
    io: &dyn Io,
) -> Result<String, String> {
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{}.3mf", crate::calibration::TIE_PRINT_NAME));
    if !output.to_ascii_lowercase().ends_with(".3mf") {
        return Err(format!("output {output}: the calibration print is a .3mf"));
    }
    let t = Instant::now();
    let mut reports = Vec::new();
    let mut fin = |c: &sf_engine::plateties::calibration::Coin| {
        let f = fin_in_place(&c.positions, engine)?;
        reports.push(f.report);
        Ok(crate::calibration::FinnedCoin {
            part: f.part,
            supports: f.fins,
        })
    };
    let print = crate::calibration::tie_print(settings, None, &mut fin)?;
    io.write(&output, &print.bytes)?;
    let total_ms = ms(t);
    if args.json {
        let num = |v: f64| Json::Num(v);
        let pairs = print
            .pairs
            .iter()
            .map(|p| {
                Json::Obj(vec![
                    ("label".into(), num(f64::from(p.variant.label))),
                    ("contactThickness".into(), num(p.variant.contact_thickness)),
                    ("middleThickness".into(), num(p.variant.middle_thickness)),
                    ("default".into(), Json::Bool(p.variant.is_default)),
                    ("requested".into(), num(p.requested as f64)),
                    ("placed".into(), num(p.placed as f64)),
                    (
                        "notes".into(),
                        Json::Arr(p.notes.iter().map(|n| Json::Str(n.clone())).collect()),
                    ),
                ])
            })
            .collect();
        return Ok(Json::Obj(vec![
            ("output".into(), Json::Str(output)),
            ("printer".into(), Json::Str(profile_id.into())),
            ("coins".into(), num(print.coins as f64)),
            ("objects".into(), num(print.objects as f64)),
            ("labelTriangles".into(), num(print.label_triangles as f64)),
            ("pairs".into(), Json::Arr(pairs)),
            ("ties".into(), num(print.placed() as f64)),
            ("tieTriangles".into(), num(print.tie_triangles as f64)),
            (
                "fins".into(),
                Json::Arr(reports.into_iter().map(Json::Str).collect()),
            ),
            ("ms".into(), num(total_ms)),
        ])
        .stringify());
    }
    for n in print.pairs.iter().flat_map(|p| &p.notes) {
        io.err(&format!("support-fins: warning: {n}"));
    }
    let mm = |v: f64| format!("{v}");
    Ok(format!(
        "plate ties calibration -> {output}: {} of {} ties; 1 = {} mm, 2 = {} mm, 3 = {} mm at the contact",
        print.placed(),
        print.requested(),
        mm(print.pairs[0].variant.contact_thickness),
        mm(print.pairs[1].variant.contact_thickness),
        mm(print.pairs[2].variant.contact_thickness),
    ))
}
