//! Unit tests for `sf_io::zip` (port of web/zip.js). Byte parity with JS is in `parity_zip.rs`.

use sf_io::zip::{ZipData, ZipFile, crc32, unzip, zip_store};

fn f(name: &str, data: &str) -> ZipFile {
    ZipFile {
        name: name.into(),
        data: ZipData::Text(data.into()),
    }
}

#[test]
fn crc32_known_values() {
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
    assert_eq!(
        crc32(b"The quick brown fox jumps over the lazy dog"),
        0x414FA339
    );
}

#[test]
fn store_then_unzip_round_trips_in_order() {
    let z = zip_store(&[
        f("b", "2"),
        f("a/x.txt", "one"),
        ZipFile {
            name: "c".into(),
            data: ZipData::Bytes(vec![0, 255]),
        },
    ]);
    let m = unzip(&z).unwrap();
    assert_eq!(m.keys().collect::<Vec<_>>(), ["b", "a/x.txt", "c"]);
    assert_eq!(m["a/x.txt"], b"one");
    assert_eq!(m["c"], vec![0, 255]);
}

#[test]
fn empty_store_is_one_eocd_record() {
    let z = zip_store(&[]);
    assert_eq!(z.len(), 22);
    assert!(unzip(&z).unwrap().is_empty());
}

#[test]
fn directory_entries_are_skipped_and_duplicates_keep_first_position() {
    let m = unzip(&zip_store(&[
        f("d/", ""),
        f("a", "1"),
        f("b", "x"),
        f("a", "2"),
    ]))
    .unwrap();
    assert_eq!(m.keys().collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(m["a"], b"2");
}

#[test]
fn inflates_a_deflate_entry() {
    // "AAAA..." as a fixed-Huffman block, built by hand: stored block is simpler
    let mut z = zip_store(&[f("s", "ABC")]);
    // turn the entry into method 8 with a stored deflate block: [1, 3,0, 252,255, 'A','B','C']
    assert!(z.len() > 40);
    let blk = [1u8, 3, 0, 252, 255, b'A', b'B', b'C'];
    let data_at = 30 + 1;
    let mut out = z[..data_at].to_vec();
    out.extend(blk);
    out.extend(&z[data_at + 3..]);
    // method 8, sizes updated in both headers
    out[8] = 8;
    out[18..22].copy_from_slice(&8u32.to_le_bytes());
    let cd = out.len() - 22 - 47;
    out[cd + 10] = 8;
    out[cd + 20..cd + 24].copy_from_slice(&8u32.to_le_bytes());
    let cd_off = cd as u32;
    let n = out.len();
    out[n - 22 + 16..n - 22 + 20].copy_from_slice(&cd_off.to_le_bytes());
    z = out;
    assert_eq!(unzip(&z).unwrap()["s"], b"ABC");
}

#[test]
fn errors_carry_the_js_messages() {
    assert_eq!(
        unzip(b"hello").unwrap_err(),
        "not a ZIP archive (no end-of-central-directory record)"
    );
    assert_eq!(
        unzip(&[]).unwrap_err(),
        "not a ZIP archive (no end-of-central-directory record)"
    );
    let mut z = zip_store(&[f("a", "x")]);
    let cd = z.len() - 22 - 47;
    z[cd] = 0;
    assert_eq!(unzip(&z).unwrap_err(), "corrupt ZIP central directory");
    let mut z = zip_store(&[f("a", "x")]);
    z[0] = 0;
    assert_eq!(unzip(&z).unwrap_err(), "corrupt local header for a");
    let mut z = zip_store(&[f("a", "x")]);
    let cd = z.len() - 22 - 47;
    z[cd + 10] = 9;
    assert_eq!(
        unzip(&z).unwrap_err(),
        "unsupported ZIP compression method 9 for a"
    );
    let mut z = zip_store(&[f("a", "x")]);
    let n = z.len();
    z[n - 22 + 16..n - 22 + 20].copy_from_slice(&1000u32.to_le_bytes());
    assert_eq!(
        unzip(&z).unwrap_err(),
        "Offset is outside the bounds of the DataView"
    );
    let mut z = zip_store(&[f("a", "x")]);
    let n = z.len();
    z[n - 22 + 10] = 0xff;
    z[n - 22 + 11] = 0xff;
    assert_eq!(
        unzip(&z).unwrap_err(),
        "corrupt ZIP: ZIP64 markers with no ZIP64 record"
    );
}

#[test]
fn trailing_comment_up_to_64k_is_found() {
    let mut z = zip_store(&[f("a", "x")]);
    let n = z.len();
    z[n - 2..].copy_from_slice(&60000u16.to_le_bytes());
    z.extend(vec![b'c'; 60000]);
    assert_eq!(unzip(&z).unwrap().len(), 1);
    z.extend(vec![b'c'; 6000]); // past 0xffff + 22 from the end: not found
    assert!(unzip(&z).is_err());
}

#[test]
fn zip_store_layout_of_one_entry() {
    let z = zip_store(&[f("a", "hi")]);
    assert_eq!(&z[..4], &[0x50, 0x4b, 3, 4]);
    assert_eq!(z.len(), 30 + 1 + 2 + 46 + 1 + 22);
    assert_eq!(
        u32::from_le_bytes(z[14..18].try_into().unwrap()),
        crc32(b"hi")
    );
    assert_eq!(u16::from_le_bytes(z[12..14].try_into().unwrap()), 0x21);
}
