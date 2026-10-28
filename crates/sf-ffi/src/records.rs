//! The JSON the app reads from a build: the readout numbers (`summary_json`) and the per-fin records
//! it selects and removes by (`records_json`). Keys follow the JS names (`web/fins.js` `buildFins`).
//!
//! `patchStats` is serialised **with its zero counters omitted** (JS leaves those keys absent, #23), and
//! a fill-pass wall has **no `region`** (JS has `undefined`; here `PropRecord.region` is `None`, #23).

use sf_cli::entry::{Stats, report_line};
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt};
use sf_engine::fins::seating::SeatingKind;
use sf_engine::prop::build::PropRecord;
use sf_engine::types::Xyz;
use sf_io::json::Json;

fn n(v: f64) -> Json {
    Json::Num(v)
}
fn u(v: usize) -> Json {
    Json::Num(v as f64)
}
fn obj(kv: Vec<(&str, Json)>) -> Json {
    Json::Obj(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn v3(p: [f64; 3]) -> Json {
    Json::Arr(p.iter().map(|&c| n(c)).collect())
}

/// `patchStats` with only the counters that counted anything.
fn patch_stats(b: &FinsBuilt) -> Json {
    let s = &b.patch_stats;
    let all = [
        ("grown", s.grown),
        ("tooSmall", s.too_small),
        ("notUpright", s.not_upright),
        ("notFlat", s.not_flat),
        ("tooShort", s.too_short),
        ("tooNarrow", s.too_narrow),
    ];
    Json::Obj(
        all.iter()
            .filter(|(_, c)| *c != 0)
            .map(|(k, c)| (k.to_string(), u(*c)))
            .collect(),
    )
}

/// What the readout shows, and the CLI's own stats and one-line report.
pub fn summary(b: &FinsBuilt, stats: &Stats, offset: Xyz) -> Json {
    let skipped = &b.skipped;
    let keys = [
        "noLine",
        "wanders",
        "stub",
        "blocked",
        "degenerate",
        "buried",
        "weld",
        "sliver",
        "onPart",
    ];
    let counts = [
        skipped.no_line,
        skipped.wanders,
        skipped.stub,
        skipped.blocked,
        skipped.degenerate,
        skipped.buried,
        skipped.weld,
        skipped.sliver,
        skipped.on_part,
    ];
    obj(vec![
        ("stats", stats.to_json()),
        ("report", Json::Str(report_line(stats))),
        ("offset", v3([offset.x, offset.y, offset.z])),
        ("mode", Json::Str(b.mode.clone())),
        ("volume", n(b.volume)),
        ("tines", u(b.tines)),
        ("braceCount", b.brace_count.map_or(Json::Null, u)),
        ("propCount", b.prop_count.map_or(Json::Null, u)),
        ("unserved", n(b.unserved)),
        ("sagRisk", Json::Bool(b.sag_risk)),
        ("smallHoleRegions", u(b.small_hole_regions)),
        ("patchCount", u(b.patch_count)),
        ("patchStats", patch_stats(b)),
        (
            "skipped",
            Json::Obj(
                keys.iter()
                    .zip(counts)
                    .filter(|(_, c)| *c != 0)
                    .map(|(k, c)| (k.to_string(), u(c)))
                    .collect(),
            ),
        ),
        (
            "rejected",
            obj(vec![
                ("blocked", u(b.rejected.blocked)),
                ("tooFewTines", u(b.rejected.too_few_tines)),
                ("sites", u(b.rejected.sites)),
                ("tried", u(b.rejected.tried)),
            ]),
        ),
        (
            "servedRegions",
            Json::Arr(b.served_regions.iter().map(|&r| u(r)).collect()),
        ),
        (
            "bare",
            b.bare.as_ref().map_or(Json::Null, |x| {
                obj(vec![
                    ("area", n(x.area)),
                    ("total", n(x.total)),
                    ("coverage", n(x.coverage)),
                ])
            }),
        ),
        (
            "floating",
            Json::Arr(
                b.floating
                    .iter()
                    .map(|p| {
                        obj(vec![
                            ("faces", u(p.faces)),
                            ("lowest", v3(p.lowest)),
                            ("drop", n(p.drop)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "sway",
            b.sway.as_ref().map_or(Json::Null, |s| {
                obj(vec![
                    ("count", u(s.count)),
                    ("tines", u(s.tines)),
                    ("skipped", u(s.skipped)),
                    ("reason", s.reason.clone().map_or(Json::Null, Json::Str)),
                    ("cross", u(s.cross)),
                ])
            }),
        ),
        ("tallRibs", b.tall_ribs.map_or(Json::Null, u)),
        // for the readouts and the next orientation ranking (ui/readout.js, ui/suggest.js)
        (
            "boreWalls",
            u(b.props.iter().filter(|q| q.in_bore == Some(true)).count()),
        ),
        (
            "seating",
            obj(vec![
                (
                    "kind",
                    Json::Str(
                        match b.seating.kind {
                            SeatingKind::Face => "face",
                            SeatingKind::Edge => "edge",
                            SeatingKind::Point => "point",
                        }
                        .into(),
                    ),
                ),
                ("span", n(b.seating.span)),
                ("bedArea", n(b.seating.bed_area)),
            ]),
        ),
        ("hasPad", Json::Bool(b.pad.is_some())),
        ("finVolumeMm3", n(mesh_volume(&b.triangles))),
        ("padVolumeMm3", n(mesh_volume(&b.pad_triangles))),
    ])
}

/// `meshVolumeMM3` (web/ui/readout.js): the volume of a closed triangle soup, mm^3.
fn mesh_volume(tris: &[sf_engine::types::V4]) -> f64 {
    let mut v = 0.0;
    for t in tris.as_chunks::<3>().0 {
        let (a, b, c) = (t[0], t[1], t[2]);
        v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    v.abs() / 6.0
}

fn kind(k: FinKind) -> &'static str {
    match k {
        FinKind::Prop => "prop",
        FinKind::Wedge => "wedge",
        FinKind::Sway => "sway",
    }
}

fn fin(f: &FinRecord) -> Json {
    let mut kv = vec![
        ("id", u(f.id)),
        ("kind", Json::Str(kind(f.kind).into())),
        ("height", n(f.height)),
        ("length", n(f.length)),
        ("tines", u(f.tines)),
        ("rows", u(f.rows)),
        ("stilt", n(f.stilt)),
        ("lean", n(f.lean)),
        ("bearing", n(f.bearing)),
        ("span", n(f.span)),
        ("short", Json::Bool(f.short)),
        ("fill", Json::Bool(f.fill)),
        ("joined", Json::Bool(f.joined)),
        // vertex ranges `[from, to)` into the `fins()` buffer
        (
            "triRanges",
            Json::Arr(
                f.tri_ranges
                    .iter()
                    .map(|r| Json::Arr(vec![u(r[0]), u(r[1])]))
                    .collect(),
            ),
        ),
        ("line", Json::Arr(f.line.iter().map(|&p| v3(p)).collect())),
    ];
    if let Some(b) = f.braces {
        kv.push(("braces", u(b)));
    }
    if let Some(r) = f.ribs {
        kv.push(("ribs", n(r)));
    }
    obj(kv)
}

fn prop(q: &PropRecord) -> Json {
    let mut kv = Vec::new();
    // a fill-pass wall has no id and no region (JS has them undefined)
    if !q.fill {
        kv.push(("id", u(q.id)));
    }
    if let Some(r) = q.region {
        kv.push(("region", u(r)));
    }
    kv.extend([
        ("tines", u(q.tines)),
        ("span", n(q.span)),
        ("height", n(q.height)),
        ("fill", Json::Bool(q.fill)),
        ("squat", Json::Bool(q.squat)),
        ("rimWall", Json::Bool(q.rim_wall)),
    ]);
    if let Some(b) = q.in_bore {
        kv.push(("inBore", Json::Bool(b)));
    }
    obj(kv)
}

/// `{ fins: [...], props: [...] }`: the unified per-fin array and the walls with their regions.
pub fn records(b: &FinsBuilt) -> Json {
    obj(vec![
        ("fins", Json::Arr(b.fins.iter().map(fin).collect())),
        ("props", Json::Arr(b.props.iter().map(prop).collect())),
        (
            "crestRanges",
            Json::Arr(
                crest_ranges(&b.triangles)
                    .into_iter()
                    .map(|[a, z]| Json::Arr(vec![u(a), u(z)]))
                    .collect(),
            ),
        ),
    ])
}

/// The interface crest as the page draws it (`meshFrom` in ui/scene.js: "one geometry group per run"): the
/// `[from, to)` vertex ranges of the runs of consecutive tagged triangles of the fin soup, a triangle
/// being the crest when its first vertex is tagged (`splitInterface`, prop/crest.js).
fn crest_ranges(tris: &[sf_engine::types::V4]) -> Vec<[usize; 2]> {
    let mut runs: Vec<[usize; 2]> = Vec::new();
    for (t, v) in tris.chunks_exact(3).enumerate() {
        if v[0][3] != 1.0 {
            continue;
        }
        let (from, to) = (t * 3, t * 3 + 3);
        match runs.last_mut() {
            Some(last) if last[1] == from => last[1] = to,
            _ => runs.push([from, to]),
        }
    }
    runs
}
