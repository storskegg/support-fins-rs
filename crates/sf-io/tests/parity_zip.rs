//! Replay of `parity/corpus/zip/{crc32,zipStore,unzip}.jsonl` (recorded by `parity/oracle/zip.js`).

use parity::{Tol, Value, compare, load_cases};
use sf_io::zip::{ZipData, ZipFile, crc32, unzip, zip_store};

#[test]
fn crc32_matches_the_oracle() {
    for c in load_cases("zip", "crc32") {
        let Value::U8(b) = &c.args[0] else {
            panic!("{}", c.id)
        };
        compare(
            &Value::from(crc32(b) as f64),
            c.out.as_ref().unwrap(),
            &Tol::Exact,
        )
        .unwrap_or_else(|m| panic!("{}: {} vs {}", c.id, m.actual, m.expected));
    }
}

#[test]
fn zip_store_matches_the_oracle_byte_for_byte() {
    let cases = load_cases("zip", "zipStore");
    assert!(cases.len() >= 10);
    for c in cases {
        let files: Vec<ZipFile> = c.args[0]
            .arr()
            .iter()
            .map(|f| ZipFile {
                name: f.get("name").str().to_string(),
                data: match f.get("data") {
                    Value::Str(s) => ZipData::Text(s.clone()),
                    Value::U8(b) => ZipData::Bytes(b.clone()),
                    o => panic!("{o:?}"),
                },
            })
            .collect();
        compare(
            &Value::U8(zip_store(&files)),
            c.out.as_ref().unwrap(),
            &Tol::Exact,
        )
        .unwrap_or_else(|m| panic!("{}: out{}: {} vs {}", c.id, m.path, m.actual, m.expected));
    }
}

#[test]
fn unzip_matches_the_oracle() {
    let cases = load_cases("zip", "unzip");
    assert!(cases.len() > 100);
    for c in cases {
        let Value::U8(bytes) = &c.args[0] else {
            panic!("{}", c.id)
        };
        match (unzip(bytes), &c.out, &c.throws) {
            (Ok(map), Some(expected), None) => {
                let actual = Value::Map(
                    map.into_iter()
                        .map(|(k, v)| (Value::Str(k), Value::U8(v)))
                        .collect(),
                );
                compare(&actual, expected, &Tol::Exact).unwrap_or_else(|m| {
                    panic!("{}: out{}: {} vs {}", c.id, m.path, m.actual, m.expected)
                });
            }
            (Err(e), None, Some(msg)) => assert_eq!(&e, msg, "{}", c.id),
            (r, out, throws) => {
                panic!(
                    "{}: Rust {:?}, JS out={} throws={:?}",
                    c.id,
                    r.map(|m| m.len()),
                    out.is_some(),
                    throws
                )
            }
        }
    }
}
