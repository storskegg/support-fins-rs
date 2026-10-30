//! Replay of `parity/corpus/swayhit/*` (recorded by `parity/oracle/swayhit.js` from web/swayhit.js and
//! web/swaycrop.js): `lowest_hit` (through the XY grid, with the rib's z bounds; the full scan agrees too) and
//! `crop_for_rib`, on the stress models. Exact.

mod common;
use ::parity::{Case, Tol, Value, compare, load_cases};
use common::nums;
use sf_engine::sway::Frame;
use sf_engine::swaycrop::crop_for_rib;
use sf_engine::swayhit::{lowest_hit, lowest_hit_brute};
use sf_engine::tris::Tris;
use sf_testkit::parity::{Resolved, resolve_args};

type V3 = [f64; 3];

fn part(a: &Resolved) -> Tris {
    let t = a.topo();
    Tris::new(t.pos[..t.n_faces * 9].to_vec())
}

#[test]
fn lowest_hit_replay() {
    let cases: Vec<Case> = load_cases("swayhit", "lowestHit");
    assert!(cases.len() > 1000);
    let (mut hits, mut misses) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let tris = part(&a[0]);
        let fr = {
            let f = a[1].plain();
            Frame {
                nh: (f.get("nh").get("x").num(), f.get("nh").get("y").num()),
                u: (f.get("uDir").get("x").num(), f.get("uDir").get("y").num()),
            }
        };
        let (u_lo, u_hi) = (a[2].plain().num(), a[3].plain().num());
        let [z0, z1, s0, slope, depth] = <[f64; 5]>::try_from(nums(a[4].plain())).unwrap();
        let o0 = move |q: &V3| q[2] - z0;
        let o1 = move |q: &V3| z1 - q[2];
        let o2 = move |q: &V3| q[0] - ((s0 + slope * q[2]) - 0.05);
        let o3 = move |q: &V3| ((s0 + slope * q[2] + depth) + 0.1) - q[0];
        let outline: [&dyn Fn(&V3) -> f64; 4] = [&o0, &o1, &o2, &o3];
        let got = lowest_hit(
            &tris,
            &fr,
            u_lo,
            u_hi,
            &outline,
            a[5].plain().num(),
            a[6].plain().num(),
            a[7].plain().num(),
            a[8].plain().num(),
        );
        let want = c.out.as_ref().unwrap();
        if let Err(m) = compare(&Value::from(got), want, &Tol::Exact) {
            panic!(
                "{}: out{} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
        let brute = lowest_hit_brute(&tris.v, &fr, u_lo, u_hi, &outline);
        assert_eq!(brute.to_bits(), got.to_bits(), "{}: scan vs grid", c.id);
        if got == f64::INFINITY {
            misses += 1;
        } else {
            hits += 1;
        }
    }
    assert!(hits > 100 && misses > 100);
}

#[test]
fn crop_for_rib_replay() {
    let cases: Vec<Case> = load_cases("swayhit", "cropForRib");
    assert!(cases.len() > 500);
    for c in &cases {
        let a = resolve_args(&c.args);
        let tris = part(&a[0]);
        let o = nums(a[1].plain());
        let lean = nums(a[3].plain());
        let got = crop_for_rib(
            &tris.v,
            [o[0], o[1]],
            a[2].plain().num(),
            [lean[0], lean[1]],
            a[4].plain().num(),
            a[5].plain().num(),
        );
        let want = c.out.as_ref().unwrap();
        if let Err(m) = compare(&Value::F64(got.v), want, &Tol::Exact) {
            panic!(
                "{}: out{} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}
