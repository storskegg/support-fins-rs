//! Port of `web/zip.js`: minimal ZIP writer (STORE only) and reader (STORE + DEFLATE).
//!
//! A .3mf is an OPC package, which is just a ZIP with a fixed set of parts. Layout per the ZIP
//! APPNOTE: a local file header + data per entry, then the central directory, then the
//! end-of-central-directory record.

use crate::text::decode;
use indexmap::IndexMap;
use miniz_oxide::{
    DataFormat, MZError, MZFlush, MZStatus,
    inflate::stream::{InflateState, inflate},
};

/// `CRC_TABLE` (web/zip.js:15-23): CRC-32 (IEEE 802.3), table built once.
fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb88320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        t[n as usize] = c;
    }
    t
}

/// Port of `crc32` (web/zip.js:25-29).
pub fn crc32(bytes: &[u8]) -> u32 {
    let table = crc_table();
    let mut c: u32 = 0xffffffff;
    for &b in bytes {
        c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffffffff
}

/// A file's `data` in `zipStore`'s input: a string (UTF-8 encoded by `TextEncoder`) or bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum ZipData {
    Text(String),
    Bytes(Vec<u8>),
}

/// One `{ name, data }` entry of `zipStore`'s input (web/zip.js:33-37). `name` uses forward
/// slashes and is stored verbatim (ASCII expected).
#[derive(Debug, Clone, PartialEq)]
pub struct ZipFile {
    pub name: String,
    pub data: ZipData,
}

fn put16(out: &mut [u8], o: usize, v: u16) {
    out[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(out: &mut [u8], o: usize, v: u32) {
    out[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// Port of `zipStore` (web/zip.js:38-107): the .zip / .3mf payload (JS returns a `Blob` of
/// these bytes).
pub fn zip_store(files: &[ZipFile]) -> Vec<u8> {
    struct E {
        name_bytes: Vec<u8>,
        bytes: Vec<u8>,
        crc: u32,
        offset: usize,
    }
    let mut entries: Vec<E> = files
        .iter()
        .map(|f| {
            let bytes = match &f.data {
                ZipData::Text(s) => s.as_bytes().to_vec(),
                ZipData::Bytes(b) => b.clone(),
            };
            E {
                name_bytes: f.name.as_bytes().to_vec(),
                crc: crc32(&bytes),
                bytes,
                offset: 0,
            }
        })
        .collect();

    // 30-byte fixed local header + name + data; 46-byte fixed central header + name.
    let mut local_size = 0;
    for e in &entries {
        local_size += 30 + e.name_bytes.len() + e.bytes.len();
    }
    let mut central_size = 0;
    for e in &entries {
        central_size += 46 + e.name_bytes.len();
    }

    let mut out = vec![0u8; local_size + central_size + 22];
    let mut o = 0;

    for e in entries.iter_mut() {
        e.offset = o;
        put32(&mut out, o, 0x04034b50); // local file header signature
        put16(&mut out, o + 4, 20); // version needed to extract
        put16(&mut out, o + 6, 0); // flags
        put16(&mut out, o + 8, 0); // method: 0 = store
        put16(&mut out, o + 10, 0); // mod time
        put16(&mut out, o + 12, 0x21); // mod date (1980-01-01, arbitrary)
        put32(&mut out, o + 14, e.crc);
        put32(&mut out, o + 18, e.bytes.len() as u32); // compressed size
        put32(&mut out, o + 22, e.bytes.len() as u32); // uncompressed size
        put16(&mut out, o + 26, e.name_bytes.len() as u16);
        put16(&mut out, o + 28, 0); // extra field length
        o += 30;
        out[o..o + e.name_bytes.len()].copy_from_slice(&e.name_bytes);
        o += e.name_bytes.len();
        out[o..o + e.bytes.len()].copy_from_slice(&e.bytes);
        o += e.bytes.len();
    }

    let central_start = o;
    for e in &entries {
        put32(&mut out, o, 0x02014b50); // central directory header signature
        put16(&mut out, o + 4, 20); // version made by
        put16(&mut out, o + 6, 20); // version needed
        put16(&mut out, o + 8, 0); // flags
        put16(&mut out, o + 10, 0); // method
        put16(&mut out, o + 12, 0); // mod time
        put16(&mut out, o + 14, 0x21); // mod date
        put32(&mut out, o + 16, e.crc);
        put32(&mut out, o + 20, e.bytes.len() as u32);
        put32(&mut out, o + 24, e.bytes.len() as u32);
        put16(&mut out, o + 28, e.name_bytes.len() as u16);
        put16(&mut out, o + 30, 0); // extra length
        put16(&mut out, o + 32, 0); // comment length
        put16(&mut out, o + 34, 0); // disk number start
        put16(&mut out, o + 36, 0); // internal attributes
        put32(&mut out, o + 38, 0); // external attributes
        put32(&mut out, o + 42, e.offset as u32); // local header offset
        o += 46;
        out[o..o + e.name_bytes.len()].copy_from_slice(&e.name_bytes);
        o += e.name_bytes.len();
    }

    put32(&mut out, o, 0x06054b50); // end of central directory signature
    put16(&mut out, o + 4, 0); // disk number
    put16(&mut out, o + 6, 0); // disk with central directory
    put16(&mut out, o + 8, entries.len() as u16);
    put16(&mut out, o + 10, entries.len() as u16);
    put32(&mut out, o + 12, central_size as u32);
    put32(&mut out, o + 16, central_start as u32);
    put16(&mut out, o + 20, 0); // comment length

    out
}

// ---------------------------------------------------------------- ZIP reader

/// The `RangeError` a `DataView` read past its end throws.
const RANGE: &str = "Offset is outside the bounds of the DataView";

/// The `DataView` over the whole archive: little-endian reads that throw like JS.
struct View<'a>(&'a [u8]);

impl View<'_> {
    fn get(&self, o: usize, n: usize) -> Result<&[u8], String> {
        match o.checked_add(n) {
            Some(e) if e <= self.0.len() => Ok(&self.0[o..e]),
            _ => Err(RANGE.to_string()),
        }
    }
    fn u16(&self, o: usize) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.get(o, 2)?.try_into().unwrap()))
    }
    fn u32(&self, o: usize) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.get(o, 4)?.try_into().unwrap()))
    }
    /// `getBigUint64` followed by `num()` (web/zip.js:139-142): throws past 2^53 - 1.
    fn num64(&self, o: usize) -> Result<usize, String> {
        let big = u64::from_le_bytes(self.get(o, 8)?.try_into().unwrap());
        if big > 9007199254740991 {
            return Err("ZIP entry is too large to read".to_string());
        }
        Ok(big as usize)
    }
}

/// Port of `findEOCD` (web/zip.js:128-135): end of central directory: fixed 22 bytes, but a
/// trailing comment (up to 0xffff) can follow, so scan back from the end for the signature.
fn find_eocd(view: &View) -> Result<Option<usize>, String> {
    let len = view.0.len();
    let max = len.min(0xffff + 22);
    let mut i = 22;
    while i <= max {
        let o = len - i;
        if view.u32(o)? == 0x06054b50 {
            return Ok(Some(o));
        }
        i += 1;
    }
    Ok(None)
}

/// Port of `findExtra` (web/zip.js:148-157): offset of the DATA of extra-field `id` within an
/// extra-field block, or `None` (JS -1). Each field is a 2-byte id, a 2-byte length, then that
/// many bytes.
fn find_extra(view: &View, start: usize, len: usize, id: u16) -> Result<Option<usize>, String> {
    let mut p = start;
    let end = start + len;
    while p + 4 <= end {
        let field_id = view.u16(p)?;
        let field_len = view.u16(p + 2)?;
        if field_id == id {
            return Ok(Some(p + 4));
        }
        p += 4 + field_len as usize;
    }
    Ok(None)
}

/// Port of `inflateRaw` (web/zip.js:160-164): `DecompressionStream('deflate-raw')` as Deno
/// ships it. Its behaviour, which this reproduces: a stream that ends early gives the bytes
/// decoded so far with no error; bytes after the end of the stream throw "failed to write
/// whole buffer"; invalid data throws "corrupt deflate stream".
fn inflate_raw(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut out: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut input = bytes;
    loop {
        let r = inflate(&mut state, input, &mut buf, MZFlush::None);
        out.extend_from_slice(&buf[..r.bytes_written]);
        input = &input[r.bytes_consumed..];
        match r.status {
            Ok(MZStatus::StreamEnd) => {
                if !input.is_empty() {
                    return Err("failed to write whole buffer".to_string());
                }
                return Ok(out);
            }
            Ok(_) => {
                if r.bytes_consumed == 0 && r.bytes_written == 0 {
                    return Ok(out);
                }
            }
            Err(MZError::Buf) => return Ok(out),
            Err(_) => return Err("corrupt deflate stream".to_string()),
        }
    }
}

/// Port of `unzip` (web/zip.js:172-238): entry name (forward slashes) -> contents, in the
/// central directory's order. Directory entries are skipped; only STORE and DEFLATE are
/// supported, which covers every 3MF in practice. JS throws (including the `RangeError`s of
/// `DataView` reads past the end); callers catch, so errors are `Err(message)`.
///
/// We walk the central directory rather than scanning for local headers: the central
/// directory is the authoritative index, and a local header may carry zeroed sizes with the
/// real ones in a trailing data descriptor.
pub fn unzip(bytes: &[u8]) -> Result<IndexMap<String, Vec<u8>>, String> {
    let view = View(bytes);
    let Some(eocd) = find_eocd(&view)? else {
        return Err("not a ZIP archive (no end-of-central-directory record)".to_string());
    };

    let mut count = view.u16(eocd + 10)? as usize;
    let mut o = view.u32(eocd + 16)? as usize;

    // ZIP64. A writer may use it for reasons of its own, not only past the 4GB /
    // 65535-entry limits -- some libraries switch it on unconditionally -- so a
    // small 3MF can arrive with 0xffff/0xffffffff placeholders in the EOCD and the
    // real values in a ZIP64 record. Refusing those rejected perfectly good files.
    // The locator sits immediately before the EOCD and points at that record.
    if eocd >= 20 && view.u32(eocd - 20)? == 0x07064b50 {
        let z64 = view.num64(eocd - 20 + 8)?;
        if z64 + 56 <= bytes.len() && view.u32(z64)? == 0x06064b50 {
            count = view.num64(z64 + 32)?;
            o = view.num64(z64 + 48)?;
        }
    }
    // Placeholders with no ZIP64 record to resolve them: the archive is malformed
    // rather than merely large, and reading on would walk garbage offsets.
    if count == 0xffff || o == 0xffffffff {
        return Err("corrupt ZIP: ZIP64 markers with no ZIP64 record".to_string());
    }

    let mut out: IndexMap<String, Vec<u8>> = IndexMap::new();
    for _ in 0..count {
        if view.u32(o)? != 0x02014b50 {
            return Err("corrupt ZIP central directory".to_string());
        }
        let method = view.u16(o + 10)?;
        let name_len = view.u16(o + 28)? as usize;
        let extra_len = view.u16(o + 30)? as usize;
        let comment_len = view.u16(o + 32)? as usize;
        let name = {
            // bytes.subarray(o + 46, o + 46 + nameLen) clamps to the archive
            let s = (o + 46).min(bytes.len());
            let e = (o + 46 + name_len).min(bytes.len());
            decode(&bytes[s..e])
        };
        let mut comp_size = view.u32(o + 20)? as usize;
        let mut local_off = view.u32(o + 42)? as usize;

        // Any 0xffffffff field is really in the entry's ZIP64 extended-information
        // extra field (id 0x0001), whose values appear in a FIXED order --
        // uncompressed, compressed, local-header offset -- but only for the fields
        // that were actually overflowed. So which ones are present is decided by
        // which placeholders we saw, not by the field's own length.
        let uncomp_placeheld = view.u32(o + 24)? == 0xffffffff;
        if uncomp_placeheld || comp_size == 0xffffffff || local_off == 0xffffffff {
            let Some(z) = find_extra(&view, o + 46 + name_len, extra_len, 0x0001)? else {
                return Err(format!(
                    "corrupt ZIP: {name} needs a ZIP64 extra field and has none"
                ));
            };
            let mut f = z;
            if uncomp_placeheld {
                f += 8; // skip uncompressed size
            }
            if comp_size == 0xffffffff {
                comp_size = view.num64(f)?;
                f += 8;
            }
            if local_off == 0xffffffff {
                local_off = view.num64(f)?;
            }
        }
        o += 46 + name_len + extra_len + comment_len;

        if name.ends_with('/') {
            continue; // directory marker
        }

        // The local header's own name/extra lengths give where the data starts --
        // the extra field routinely differs in length from the central one.
        if view.u32(local_off)? != 0x04034b50 {
            return Err(format!("corrupt local header for {name}"));
        }
        let l_name_len = view.u16(local_off + 26)? as usize;
        let l_extra_len = view.u16(local_off + 28)? as usize;
        let start = local_off + 30 + l_name_len + l_extra_len;
        // bytes.subarray(start, start + compSize) clamps to the archive
        let s = start.min(bytes.len());
        let e = (start + comp_size).min(bytes.len());
        let data = &bytes[s..e.max(s)];

        if method == 0 {
            out.insert(name, data.to_vec());
        } else if method == 8 {
            out.insert(name, inflate_raw(data)?);
        } else {
            return Err(format!(
                "unsupported ZIP compression method {method} for {name}"
            ));
        }
    }
    Ok(out)
}
