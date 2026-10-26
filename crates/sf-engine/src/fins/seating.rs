//! Port of `web/fins/seating.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! How the part sits on the plate: `seated_part_tris` lays the part's triangles out in print
//! space, `bed_contact` finds where it touches down (and its centre of mass), and `seating_of`
//! calls that contact a face, an edge or a point.

use std::sync::Arc;

use crate::jsnum;
use crate::overhangs::{Analysis, BED_EPS, Topology};
use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// The pose key `` `${rot.join(',')}|${offset.x},${offset.y},${offset.z}` `` (web/fins/seating.js:24,
/// web/prop/nearface.js:51), built with `to_js_string` because it decides cache hits exactly as JS does.
pub(crate) fn pose_key(rot: &Rot3, offset: &Xyz) -> String {
    let rot: Vec<String> = rot.iter().map(|&v| jsnum::to_js_string(v)).collect();
    format!(
        "{}|{},{},{}",
        rot.join(","),
        jsnum::to_js_string(offset.x),
        jsnum::to_js_string(offset.y),
        jsnum::to_js_string(offset.z)
    )
}

/// Port of `seatedPartTris` (web/fins/seating.js:23-31): the whole part, seated into print
/// space, as a flat triangle array. The same array comes back for the same pose (callers only
/// read it), so the surface grid cached on it is built once per pose, not once per caller.
/// Cached on the topology under the same key string JS builds.
pub fn seated_part_tris(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Arc<Tris> {
    let key = pose_key(rot, offset);
    let mut slot = topo.cache.seated.lock().unwrap();
    if let Some((k, tris)) = slot.as_ref()
        && *k == key
    {
        return tris.clone();
    }
    let tris = Arc::new(Tris::new(seat(topo, rot, offset)));
    *slot = Some((key, tris.clone()));
    tris
}

/// Port of `forgetSeated` (web/fins/seating.js:34): drop the cached copy.
pub fn forget_seated(topo: &Topology) {
    *topo.cache.seated.lock().unwrap() = None;
}

/// Port of `seat` (web/fins/seating.js:36-49).
fn seat(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Vec<f64> {
    let (pos, n_faces) = (&topo.pos, topo.n_faces);
    let (ox, oy, oz) = (offset.x, offset.y, offset.z);
    let mut tris = vec![0.0f64; n_faces * 9];
    for f in 0..n_faces {
        for i in 0..3 {
            let o = f * 9 + i * 3;
            let (x, y, z) = (pos[o], pos[o + 1], pos[o + 2]);
            tris[o] = rot[0] * x + rot[3] * y + rot[6] * z + ox;
            tris[o + 1] = rot[1] * x + rot[4] * y + rot[7] * z + oy;
            tris[o + 2] = rot[2] * x + rot[5] * y + rot[8] * z + oz;
        }
    }
    tris
}

/// The object `bedContact` returns (web/fins/seating.js:73): `{ pts, mx, my }`.
#[derive(Clone, Debug, PartialEq)]
pub struct BedContact {
    /// Contact points `[x, y]`.
    pub pts: Vec<[f64; 2]>,
    pub mx: f64,
    pub my: f64,
}

/// Port of `bedContact` (web/fins/seating.js:59-77): the part's bed-contact points and its
/// area-weighted centre of mass.
///
/// Contact comes from VERTICES under BED_EPS, not from bed-flagged faces: a part tilted onto an
/// edge has no face on the plate at all, which is exactly the case the bed pad exists for.
pub fn bed_contact(topo: &Topology, result: &Analysis, rot: &Rot3) -> BedContact {
    let (pos, n_faces, area) = (&topo.pos, topo.n_faces, &topo.area);
    let (ox, oy, oz) = (result.offset.x, result.offset.y, result.offset.z);
    let (mut mx, mut my, mut mw) = (0.0f64, 0.0f64, 0.0f64);
    let mut pts = Vec::new();
    for f in 0..n_faces {
        let (mut gx, mut gy) = (0.0f64, 0.0f64);
        for i in 0..3 {
            let o = f * 9 + i * 3;
            let (x, y, z) = (pos[o], pos[o + 1], pos[o + 2]);
            let wx = rot[0] * x + rot[3] * y + rot[6] * z + ox;
            let wy = rot[1] * x + rot[4] * y + rot[7] * z + oy;
            let wz = rot[2] * x + rot[5] * y + rot[8] * z + oz;
            gx += wx;
            gy += wy;
            if wz < BED_EPS {
                pts.push([wx, wy]);
            }
        }
        mx += (gx / 3.0) * area[f];
        my += (gy / 3.0) * area[f];
        mw += area[f];
    }
    if mw > 0.0 {
        mx /= mw;
        my /= mw;
    }
    BedContact { pts, mx, my }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::overhangs::TopoCache;
    use crate::types::IDENTITY3;

    /// A topology of `pos` with only what seating reads (`pos`, `n_faces`, `area`).
    pub(crate) fn topo_of(pos: Vec<f64>) -> Topology {
        let n_faces = pos.len() / 9;
        let area = (0..n_faces)
            .map(|f| {
                let o = f * 9;
                let (ux, uy, uz) = (
                    pos[o + 3] - pos[o],
                    pos[o + 4] - pos[o + 1],
                    pos[o + 5] - pos[o + 2],
                );
                let (vx, vy, vz) = (
                    pos[o + 6] - pos[o],
                    pos[o + 7] - pos[o + 1],
                    pos[o + 8] - pos[o + 2],
                );
                let (px, py, pz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
                0.5 * jsnum::hypot(&[px, py, pz])
            })
            .collect();
        Topology {
            pos,
            n_faces,
            nrm: vec![],
            area,
            vid: vec![],
            adj_a: vec![],
            adj_b: vec![],
            vertex_count: 0,
            edge_count: 0,
            cache: TopoCache::default(),
        }
    }

    const TRI: [f64; 9] = [0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 3.0, 0.0];
    const OFF: Xyz = Xyz {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };

    #[test]
    fn pose_key_is_the_js_template_string() {
        assert_eq!(pose_key(&IDENTITY3, &OFF), "1,0,0,0,1,0,0,0,1|1,2,3");
        // -0 prints "0", fractions and exponents print as JS does
        let rot = [-0.0, 0.5, 1e21, 1e-7, 0.1 + 0.2, 0.0, 0.0, 0.0, 1.0];
        let off = Xyz {
            x: -0.0,
            y: 1.5,
            z: f64::NAN,
        };
        assert_eq!(
            pose_key(&rot, &off),
            "0,0.5,1e+21,1e-7,0.30000000000000004,0,0,0,1|0,1.5,NaN"
        );
    }

    #[test]
    fn seats_every_vertex() {
        let t = topo_of(TRI.to_vec());
        let rot = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let s = seated_part_tris(&t, &rot, &OFF);
        // x' = rot[0]x + rot[3]y + rot[6]z + ox = -y + 1, y' = x + 2, z' = z + 3
        assert_eq!(s.v, vec![1.0, 2.0, 3.0, 1.0, 6.0, 3.0, -2.0, 2.0, 3.0]);
    }

    #[test]
    fn same_pose_returns_the_same_array_until_the_pose_changes_or_it_is_forgotten() {
        let t = topo_of(TRI.to_vec());
        let a = seated_part_tris(&t, &IDENTITY3, &OFF);
        assert!(Arc::ptr_eq(&a, &seated_part_tris(&t, &IDENTITY3, &OFF)));
        // a different offset is a different pose: a new array, and only the last pose is kept
        let b = seated_part_tris(&t, &IDENTITY3, &Xyz { z: 4.0, ..OFF });
        assert!(!Arc::ptr_eq(&a, &b));
        let a2 = seated_part_tris(&t, &IDENTITY3, &OFF);
        assert!(!Arc::ptr_eq(&a, &a2));
        assert_eq!(a.v, a2.v);
        forget_seated(&t);
        assert!(!Arc::ptr_eq(&a2, &seated_part_tris(&t, &IDENTITY3, &OFF)));
        // another topology has its own cache
        let u = topo_of(TRI.to_vec());
        assert!(!Arc::ptr_eq(&a2, &seated_part_tris(&u, &IDENTITY3, &OFF)));
    }

    #[test]
    fn an_empty_part_seats_to_an_empty_soup() {
        let t = topo_of(vec![]);
        assert!(seated_part_tris(&t, &IDENTITY3, &OFF).is_empty());
    }

    fn analysis(offset: Xyz) -> Analysis {
        Analysis {
            over: vec![],
            kept: vec![],
            on_bed: vec![],
            regions: vec![],
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: 0.0,
            over_face_count: 0,
            offset,
            size: Xyz::default(),
        }
    }

    #[test]
    fn bed_contact_takes_vertices_under_bed_eps_and_the_area_weighted_centre() {
        // z = 0 and z = 0.34 are contact (< BED_EPS 0.35); 0.35 itself and 5 are not
        let pos = vec![
            0.0, 0.0, 0.0, 4.0, 0.0, 0.34, 0.0, 3.0, 5.0, //
            10.0, 0.0, 0.35, 11.0, 0.0, 0.35, 10.0, 1.0, 0.35,
        ];
        let t = topo_of(pos);
        let bc = bed_contact(&t, &analysis(Xyz::default()), &IDENTITY3);
        assert_eq!(bc.pts, vec![[0.0, 0.0], [4.0, 0.0]]);
        let (a0, a1) = (t.area[0], t.area[1]);
        let mx = ((0.0 + 4.0 + 0.0) / 3.0 * a0 + (10.0 + 11.0 + 10.0) / 3.0 * a1) / (a0 + a1);
        assert_eq!(bc.mx.to_bits(), mx.to_bits());
    }

    #[test]
    fn bed_contact_with_no_area_leaves_the_centre_at_zero() {
        // a degenerate face has zero area: mw = 0, so mx and my stay 0 (not NaN)
        let t = topo_of(vec![1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0]);
        let bc = bed_contact(&t, &analysis(Xyz::default()), &IDENTITY3);
        assert_eq!((bc.mx, bc.my), (0.0, 0.0));
        let t = topo_of(vec![]);
        assert_eq!(
            bed_contact(&t, &analysis(Xyz::default()), &IDENTITY3)
                .pts
                .len(),
            0
        );
    }

    #[test]
    fn bed_contact_applies_rotation_and_offset() {
        let t = topo_of(TRI.to_vec());
        // offset z lifts the face off the bed: no contact
        let up = analysis(Xyz {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        });
        assert!(bed_contact(&t, &up, &IDENTITY3).pts.is_empty());
        let on = analysis(Xyz {
            x: 1.0,
            y: 1.0,
            z: 0.0,
        });
        assert_eq!(
            bed_contact(&t, &on, &IDENTITY3).pts,
            vec![[1.0, 1.0], [5.0, 1.0], [1.0, 4.0]]
        );
    }
}
