//! Replays `parity/corpus/footprint/*` (recorded from web/ui/footprint.js and web/ui/platerow.js by
//! `parity/oracle/footprint.js`). Grids are integers and discrete bytes: every comparison is exact.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::footprint::{
    Grid, area, bed_grid, dilate, drag_step, find_spot, pack, placed, plate_under, rasterize,
    shared, take, union,
};

fn int(v: &Value) -> i64 {
    v.num() as i64
}

fn xy(v: &Value) -> [f64; 2] {
    let a = v.arr();
    [a[0].num(), a[1].num()]
}

fn grid(v: &Value) -> Option<Grid> {
    if matches!(v, Value::Null) {
        return None;
    }
    let bits = match v.get("bits") {
        Value::U8(b) => b.clone(),
        o => panic!("bits is {o:?}"),
    };
    let bed = match v {
        Value::Obj(o) if o.iter().any(|(k, _)| k == "bx") => {
            Some((v.get("bx").num(), v.get("by").num()))
        }
        _ => None,
    };
    Some(Grid {
        x0: int(v.get("x0")),
        y0: int(v.get("y0")),
        w: int(v.get("w")),
        h: int(v.get("h")),
        bits,
        bed,
    })
}

/// A grid as the JS holds it, for `compare` (a bed carries its `bx` and `by` last).
fn value(g: &Option<Grid>) -> Value {
    let Some(g) = g else { return Value::Null };
    let mut o = vec![
        ("x0".to_string(), Value::from(g.x0 as f64)),
        ("y0".to_string(), Value::from(g.y0 as f64)),
        ("w".to_string(), Value::from(g.w as f64)),
        ("h".to_string(), Value::from(g.h as f64)),
        ("bits".to_string(), Value::U8(g.bits.clone())),
    ];
    if let Some((bx, by)) = g.bed {
        o.push(("bx".to_string(), bx.into()));
        o.push(("by".to_string(), by.into()));
    }
    Value::Obj(o)
}

fn check(c: &Case, actual: &Value) {
    compare(actual, c.out.as_ref().unwrap(), &Tol::Exact).unwrap_or_else(|m| {
        panic!(
            "{}: out{}: got {}, expected {}",
            c.id, m.path, m.actual, m.expected
        )
    });
}

fn pair(a: [i64; 2]) -> Value {
    Value::Arr(vec![(a[0] as f64).into(), (a[1] as f64).into()])
}

fn spot(s: Option<[i64; 2]>) -> Value {
    s.map_or(Value::Null, pair)
}

#[test]
fn rasterize_matches_the_oracle() {
    let cases = load_cases("footprint", "rasterize");
    assert!(cases.len() > 200);
    for c in &cases {
        let tris: Vec<f64> = match &c.args[0] {
            Value::F32(a) => a.iter().map(|&x| x as f64).collect(),
            o => panic!("{o:?}"),
        };
        let m: Option<[f64; 16]> = match &c.args[1] {
            Value::F64(a) => Some(a.as_slice().try_into().unwrap()),
            Value::Null => None,
            o => panic!("{o:?}"),
        };
        check(c, &value(&rasterize(&tris, m.as_ref())));
    }
}

#[test]
fn union_matches_the_oracle() {
    let cases = load_cases("footprint", "union");
    assert!(cases.len() > 100);
    for c in &cases {
        let gs: Vec<Option<Grid>> = c.args[0].arr().iter().map(grid).collect();
        let refs: Vec<Option<&Grid>> = gs.iter().map(Option::as_ref).collect();
        check(c, &value(&union(&refs)));
    }
}

#[test]
fn placed_matches_the_oracle() {
    let cases = load_cases("footprint", "placed");
    assert!(cases.len() > 200);
    for c in &cases {
        let g = grid(&c.args[0]).unwrap();
        check(c, &value(&Some(placed(&g, xy(&c.args[1])))));
    }
}

#[test]
fn shared_matches_the_oracle() {
    let cases = load_cases("footprint", "shared");
    assert!(cases.len() > 200);
    for c in &cases {
        let (a, b) = (grid(&c.args[0]), grid(&c.args[1]));
        let cells = shared(a.as_ref(), b.as_ref());
        check(c, &Value::Arr(cells.into_iter().map(pair).collect()));
    }
}

#[test]
fn dilate_and_area_match_the_oracle() {
    for c in &load_cases("footprint", "dilate") {
        let g = grid(&c.args[0]).unwrap();
        check(c, &value(&Some(dilate(&g, int(&c.args[1])))));
    }
    for c in &load_cases("footprint", "area") {
        check(c, &Value::from(area(grid(&c.args[0]).as_ref()) as f64));
    }
}

#[test]
fn bed_grid_and_take_match_the_oracle() {
    for c in &load_cases("footprint", "bedGrid") {
        check(c, &value(&Some(bed_grid(c.args[0].num(), c.args[1].num()))));
    }
    let cases = load_cases("footprint", "take");
    assert!(cases.len() > 50);
    for c in &cases {
        let mut bed = grid(&c.args[0]).unwrap();
        take(&mut bed, grid(&c.args[1]).as_ref());
        check(c, &value(&Some(bed)));
    }
}

#[test]
fn find_spot_matches_the_oracle() {
    let cases = load_cases("footprint", "findSpot");
    assert!(cases.len() > 100);
    for c in &cases {
        let (bed, g) = (grid(&c.args[0]).unwrap(), grid(&c.args[1]).unwrap());
        check(
            c,
            &spot(find_spot(&bed, &g, int(&c.args[2]), int(&c.args[3]))),
        );
    }
}

#[test]
fn pack_matches_the_oracle() {
    let cases = load_cases("footprint", "pack");
    assert!(cases.len() > 50);
    for c in &cases {
        let gs: Vec<Option<Grid>> = c.args[0].arr().iter().map(grid).collect();
        let refs: Vec<Option<&Grid>> = gs.iter().map(Option::as_ref).collect();
        let at = pack(&refs, c.args[1].num(), c.args[2].num(), int(&c.args[3]));
        check(c, &Value::Arr(at.into_iter().map(spot).collect()));
    }
}

#[test]
fn drag_step_matches_the_oracle() {
    let cases = load_cases("footprint", "dragStep");
    assert!(cases.len() > 100);
    for c in &cases {
        let me = grid(&c.args[2]).unwrap();
        let others: Vec<Grid> = c.args[3].arr().iter().map(|g| grid(g).unwrap()).collect();
        let clash = |p: [f64; 2]| {
            let at = placed(&me, p);
            others
                .iter()
                .map(|o| shared(Some(&at), Some(o)).len())
                .sum::<usize>() as f64
        };
        let out = drag_step(xy(&c.args[0]), xy(&c.args[1]), clash);
        check(c, &Value::Arr(vec![out[0].into(), out[1].into()]));
    }
}

#[test]
fn plate_under_matches_the_oracle() {
    let cases = load_cases("footprint", "plateUnder");
    assert!(cases.len() > 100);
    for c in &cases {
        let a = &c.args;
        let got = plate_under(
            xy(&a[0]),
            a[1].num(),
            a[2].num(),
            a[3].num(),
            a[4].num(),
            a[5].num(),
        );
        let v = got.map_or(Value::Null, |(plate, at)| {
            Value::Obj(vec![
                ("plate".to_string(), (plate as f64).into()),
                (
                    "xy".to_string(),
                    Value::Arr(vec![at[0].into(), at[1].into()]),
                ),
            ])
        });
        check(c, &v);
    }
}
