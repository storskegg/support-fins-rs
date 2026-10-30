//! Safe writes (audit X-08, X-10, S-11): zip entry names, and the ZIP32 and STL limits.
use sf_io::stl::{stl_count_check, write_binary_stl, write_binary_stl_checked};
use sf_io::zip::{
    ZipData, ZipFile, safe_entry_names, unzip, zip_store, zip_store_checked, zip32_check,
};

fn names(v: &[&str]) -> Vec<String> {
    safe_entry_names(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

#[test]
fn normal_names_are_unchanged() {
    let ok = [
        "plate-1-Benchy-fins-0.4nozzle-0.2layer.3mf",
        "plate-2-Café mug-and-1-more-fins-0.4nozzle-0.2layer.3mf",
        "support-fins-plates.zip",
        "a.b.c.3mf",
    ];
    assert_eq!(names(&ok), ok);
}

#[test]
fn traversal_separators_and_control_characters_are_gone() {
    let got = names(&[
        "plate-1-x/../../../../Library/LaunchAgents/evil-fins.3mf",
        "..\\..\\evil.3mf",
        "../../etc/passwd",
        "a\u{0}b\nc\u{7f}.3mf",
        "C:evil.3mf",
        "",
        "..",
    ]);
    for n in &got {
        assert!(
            !n.contains('/') && !n.contains('\\') && !n.contains("..") && !n.contains(':'),
            "{n}"
        );
        assert!(!n.chars().any(|c| c.is_control()), "{n:?}");
        assert!(!n.is_empty());
    }
    assert_eq!(
        got[0],
        "plate-1-x---------Library-LaunchAgents-evil-fins.3mf"
    );
}

#[test]
fn names_are_unique_without_regard_to_case_and_keep_their_extension() {
    let got = names(&["a.3mf", "a.3mf", "A.3MF", "a/b.3mf", "a-b.3mf"]);
    assert_eq!(got, ["a.3mf", "a-2.3mf", "A-3.3MF", "a-b.3mf", "a-b-2.3mf"]);
}

#[test]
fn a_long_name_is_cut_with_its_extension_kept() {
    let long = format!("{}.3mf", "x".repeat(70_000));
    let got = names(&[&long]);
    assert!(
        got[0].len() <= 200 && got[0].ends_with(".3mf"),
        "{}",
        got[0].len()
    );
}

#[test]
fn a_traversal_named_zip_reads_back_with_safe_entries() {
    let safe = names(&["plate-1-x/../../../evil.3mf", "plate-1-x/../../../evil.3mf"]);
    let files: Vec<ZipFile> = safe
        .into_iter()
        .map(|name| ZipFile {
            name,
            data: ZipData::Bytes(vec![1, 2, 3]),
        })
        .collect();
    let back = unzip(&zip_store_checked(&files).unwrap()).unwrap();
    assert_eq!(back.len(), 2);
    assert!(back.keys().all(|k| !k.contains('/') && !k.contains("..")));
}

#[test]
fn the_checked_zip_is_the_unchecked_zips_bytes() {
    let files = vec![ZipFile {
        name: "a".into(),
        data: ZipData::Text("hello".into()),
    }];
    assert_eq!(zip_store_checked(&files).unwrap(), zip_store(&files));
}

#[test]
fn zip32_limits_are_refused_with_a_message() {
    // a name over 65,535 bytes
    let long = vec![ZipFile {
        name: "n".repeat(65_536),
        data: ZipData::Bytes(vec![]),
    }];
    let e = zip_store_checked(&long).unwrap_err();
    assert!(e.contains("65536") && e.contains("65535"), "{e}");
    // more than 65,535 entries
    let many: Vec<ZipFile> = (0..65_536)
        .map(|i| ZipFile {
            name: format!("{i}"),
            data: ZipData::Bytes(vec![]),
        })
        .collect();
    assert!(
        zip_store_checked(&many)
            .unwrap_err()
            .contains("65536 files")
    );
    // exactly 65,535 is fine
    assert!(zip_store_checked(&many[..65_535]).is_ok());
    // over 4 GB (sizes no machine need hold)
    let e = zip32_check(2, [(5, 3_000_000_000u64), (5, 3_000_000_000)].into_iter()).unwrap_err();
    assert!(e.contains("4 GB"), "{e}");
    assert!(zip32_check(1, [(5, 4_000_000_000u64)].into_iter()).is_ok());
}

#[test]
fn the_stl_triangle_count_must_fit_its_uint32() {
    assert!(stl_count_check(u32::MAX as usize).is_ok());
    let e = stl_count_check(u32::MAX as usize + 1).unwrap_err();
    assert!(e.contains("4294967296") && e.contains("4294967295"), "{e}");
    let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    assert_eq!(
        write_binary_stl_checked(&tri, "t").unwrap(),
        write_binary_stl(&tri, "t")
    );
}
