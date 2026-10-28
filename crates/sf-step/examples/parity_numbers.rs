//! `cargo run -p sf-step --example parity_numbers`: for every oracle input, how the native tessellation differs from
//! the web's: vertex and triangle counts, the largest coordinate difference, the indices that differ.
use parity::{Value, load_cases};

fn field<'a>(v: &'a Value, k: &str) -> &'a Value {
    let Value::Obj(f) = v else {
        panic!("not an object")
    };
    &f.iter().find(|(n, _)| n == k).expect("field").1
}

fn main() {
    let (mut meshes, mut differing, mut verts, mut tris) = (0, 0, 0, 0);
    for c in load_cases("step", "readStepFile") {
        let Value::U8(bytes) = &c.args[0] else {
            panic!("arg")
        };
        let expected = c.out.as_ref().expect("out");
        let raw = sf_step::read_raw(bytes).expect("kernel");
        let (Some(raw), Value::Bool(true)) = (raw, field(expected, "success")) else {
            println!("{:26} the kernel read nothing (both)", c.id);
            continue;
        };
        let Value::Arr(web) = field(expected, "meshes") else {
            panic!("meshes")
        };
        for (i, m) in web.iter().enumerate() {
            let (Value::F32(wp), Value::U32(wi)) = (field(m, "position"), field(m, "index")) else {
                panic!("mesh")
            };
            let r = &raw.meshes[i];
            let max_d = wp
                .iter()
                .zip(&r.positions)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            let idx_diff = wi.iter().zip(&r.indices).filter(|(a, b)| a != b).count();
            let same_shape = wp.len() == r.positions.len() && wi.len() == r.indices.len();
            meshes += 1;
            verts += wp.len() / 3;
            tris += wi.len() / 3;
            if !same_shape || max_d != 0.0 || idx_diff != 0 {
                differing += 1;
            }
            println!(
                "{:26} mesh {i}: {:6} verts {:6} tris  same counts {same_shape}  max |dxyz| {max_d:e}  indices differing {idx_diff}",
                c.id,
                wp.len() / 3,
                wi.len() / 3
            );
        }
    }
    println!("{meshes} meshes, {verts} vertices, {tris} triangles; {differing} meshes differ");
}
