//! Port of `web/zip.js`: minimal ZIP writer (STORE only) and reader (STORE + DEFLATE).
//!
//! A .3mf is an OPC package, which is just a ZIP with a fixed set of parts. Layout per the ZIP
//! APPNOTE: a local file header + data per entry, then the central directory, then the
//! end-of-central-directory record.

use crate::limits::{LimitExceeded, LoadOptions, MAX_DECOMPRESSED_BYTES, ReadError};
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
///
/// The common case (a well-formed stream that ends where the entry does) goes through zlib-rs,
/// about twice as fast as miniz_oxide; every other outcome (truncated or corrupt data, bytes
/// after the end of the stream) is re-run through the miniz_oxide code below, which defines
/// those behaviours, so they cannot differ.
#[cfg(test)]
fn inflate_raw(bytes: &[u8], size_hint: usize) -> Result<Vec<u8>, String> {
    inflate_capped(bytes, size_hint, usize::MAX).map_err(|e| match e {
        Capped::Over => unreachable!("no cap"),
        Capped::Err(m) => m,
    })
}

/// Why a capped inflate stopped.
enum Capped {
    /// more than the cap came out
    Over,
    Err(String),
}

/// `inflate_raw` that stops (with `Capped::Over`) as soon as more than `cap` bytes have come out.
fn inflate_capped(bytes: &[u8], size_hint: usize, cap: usize) -> Result<Vec<u8>, Capped> {
    match inflate_fast(bytes, size_hint, cap)? {
        Some(v) => Ok(v),
        None => inflate_miniz_capped(bytes, cap),
    }
}

/// A clean inflate of the whole of `bytes`, or `None`.
fn inflate_fast(bytes: &[u8], size_hint: usize, cap: usize) -> Result<Option<Vec<u8>>, Capped> {
    use flate2::{Decompress, FlushDecompress, Status};
    // deflate cannot expand more than 1032:1, so the hint can't make this reserve absurd
    let want = size_hint
        .min(bytes.len().saturating_mul(1032))
        .min(cap.saturating_add(1));
    let mut out: Vec<u8> = Vec::with_capacity(want.max(64 * 1024));
    let mut d = Decompress::new(false);
    let mut input = bytes;
    loop {
        if out.len() == out.capacity() {
            out.reserve(out.len().max(64 * 1024));
        }
        let (in0, out0) = (d.total_in(), out.len());
        let status = d.decompress_vec(input, &mut out, FlushDecompress::None);
        if out.len() > cap {
            return Err(Capped::Over);
        }
        let consumed = (d.total_in() - in0) as usize;
        input = &input[consumed..];
        match status {
            Ok(Status::StreamEnd) => return Ok(input.is_empty().then_some(out)),
            Ok(_) if consumed > 0 || out.len() > out0 => {}
            _ => return Ok(None),
        }
    }
}

/// `inflate_raw` as miniz_oxide runs it: the definition of the edge-case behaviour.
#[cfg(test)]
fn inflate_raw_miniz(bytes: &[u8]) -> Result<Vec<u8>, String> {
    inflate_miniz_capped(bytes, usize::MAX).map_err(|e| match e {
        Capped::Over => unreachable!("no cap"),
        Capped::Err(m) => m,
    })
}

fn inflate_miniz_capped(bytes: &[u8], cap: usize) -> Result<Vec<u8>, Capped> {
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut out: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut input = bytes;
    loop {
        let r = inflate(&mut state, input, &mut buf, MZFlush::None);
        out.extend_from_slice(&buf[..r.bytes_written]);
        if out.len() > cap {
            return Err(Capped::Over);
        }
        input = &input[r.bytes_consumed..];
        match r.status {
            Ok(MZStatus::StreamEnd) => {
                if !input.is_empty() {
                    return Err(Capped::Err("failed to write whole buffer".to_string()));
                }
                return Ok(out);
            }
            Ok(_) => {
                if r.bytes_consumed == 0 && r.bytes_written == 0 {
                    return Ok(out);
                }
            }
            Err(MZError::Buf) => return Ok(out),
            Err(_) => return Err(Capped::Err("corrupt deflate stream".to_string())),
        }
    }
}

/// One central-directory record, resolved: where its data is and what it claims to unpack to.
struct Entry {
    name: String,
    method: u16,
    /// where the compressed data starts and how long it says it is
    start: usize,
    comp_size: usize,
    local_off: usize,
    /// the central record's uncompressed size (a claim, checked against the real inflate)
    declared: u64,
}

/// Port of `unzip` (web/zip.js:172-238): entry name (forward slashes) -> contents, in the
/// central directory's order. Directory entries are skipped; only STORE and DEFLATE are
/// supported, which covers every 3MF in practice. JS throws (including the `RangeError`s of
/// `DataView` reads past the end); callers catch, so errors are `Err(message)`.
///
/// We walk the central directory rather than scanning for local headers: the central
/// directory is the authoritative index, and a local header may carry zeroed sizes with the
/// real ones in a trailing data descriptor.
///
/// Hostile archives (audit X-04): overlapping entries are refused outright, and the entries'
/// total unpacked size is held to [`MAX_DECOMPRESSED_BYTES`] (declared sizes are checked before
/// anything is inflated; a running budget catches a stream that lies about its size). Use
/// [`unzip_with`] for `allow_large`.
pub fn unzip(bytes: &[u8]) -> Result<IndexMap<String, Vec<u8>>, String> {
    unzip_with(bytes, &LoadOptions::default()).map_err(String::from)
}

/// [`unzip`] with the load options: `allow_large` lifts the 2 GiB total (overlap is still refused).
pub fn unzip_with(
    bytes: &[u8],
    opts: &LoadOptions,
) -> Result<IndexMap<String, Vec<u8>>, ReadError> {
    let view = View(bytes);
    let Some(eocd) = find_eocd(&view)? else {
        return Err("not a ZIP archive (no end-of-central-directory record)"
            .to_string()
            .into());
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
        return Err("corrupt ZIP: ZIP64 markers with no ZIP64 record"
            .to_string()
            .into());
    }

    // Pass 1: the whole central directory, resolved. Nothing is inflated yet.
    let mut entries: Vec<Entry> = Vec::new();
    for _ in 0..count {
        if view.u32(o)? != 0x02014b50 {
            return Err("corrupt ZIP central directory".to_string().into());
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
        let mut declared = view.u32(o + 24)? as u64; // 0xffffffff when ZIP64: read below
        let mut local_off = view.u32(o + 42)? as usize;

        // Any 0xffffffff field is really in the entry's ZIP64 extended-information
        // extra field (id 0x0001), whose values appear in a FIXED order --
        // uncompressed, compressed, local-header offset -- but only for the fields
        // that were actually overflowed. So which ones are present is decided by
        // which placeholders we saw, not by the field's own length.
        let uncomp_placeheld = view.u32(o + 24)? == 0xffffffff;
        if uncomp_placeheld || comp_size == 0xffffffff || local_off == 0xffffffff {
            let Some(z) = find_extra(&view, o + 46 + name_len, extra_len, 0x0001)? else {
                return Err(
                    format!("corrupt ZIP: {name} needs a ZIP64 extra field and has none").into(),
                );
            };
            let mut f = z;
            if uncomp_placeheld {
                declared = u64::from_le_bytes(view.get(f, 8)?.try_into().unwrap());
                f += 8;
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
            return Err(format!("corrupt local header for {name}").into());
        }
        let l_name_len = view.u16(local_off + 26)? as usize;
        let l_extra_len = view.u16(local_off + 28)? as usize;
        let start = local_off + 30 + l_name_len + l_extra_len;
        if method != 0 && method != 8 {
            return Err(format!("unsupported ZIP compression method {method} for {name}").into());
        }
        entries.push(Entry {
            name,
            method,
            start,
            comp_size,
            local_off,
            declared,
        });
    }

    // Entries must not share bytes: an archive whose records point at one stream makes the
    // inflated total many times the file (a 1 MB archive of 4 GiB). Two records with one local
    // header always share; otherwise ranges that lie inside the archive must not overlap (a size
    // that runs off the end is a truncated entry, left to the inflate, as the JS does).
    let mut by_off: Vec<(usize, usize)> = entries
        .iter()
        .map(|e| (e.local_off, e.start.saturating_add(e.comp_size)))
        .collect();
    by_off.sort_unstable();
    if by_off
        .windows(2)
        .any(|w| w[1].0 == w[0].0 || (w[0].1 <= bytes.len() && w[1].0 < w[0].1))
    {
        return Err("corrupt ZIP: entries overlap".to_string().into());
    }

    // What the entries claim to unpack to, before any of it is inflated. Deflate cannot
    // expand more than 1032:1, so a larger claim is capped at what the data could give.
    let claimed = |e: &Entry| -> u64 {
        let avail = e.comp_size.min(bytes.len().saturating_sub(e.start)) as u64;
        if e.method == 0 {
            avail
        } else {
            e.declared.min(avail.saturating_mul(1032))
        }
    };
    let total_claim: u64 = entries.iter().map(claimed).fold(0, u64::saturating_add);
    if !opts.allow_large && total_claim > MAX_DECOMPRESSED_BYTES {
        return Err(LimitExceeded::bytes(total_claim).into());
    }

    // Pass 2: inflate, with a running budget for a stream that outgrows its claim.
    let mut out: IndexMap<String, Vec<u8>> = IndexMap::new();
    let mut spent: u64 = 0;
    for e in entries {
        // bytes.subarray(start, start + compSize) clamps to the archive
        let s = e.start.min(bytes.len());
        let end = e.start.saturating_add(e.comp_size).min(bytes.len());
        let data = &bytes[s..end.max(s)];
        let cap = if opts.allow_large {
            usize::MAX
        } else {
            (MAX_DECOMPRESSED_BYTES - spent) as usize
        };
        let got = if e.method == 0 {
            if data.len() > cap {
                return Err(
                    LimitExceeded::bytes(total_claim.max(spent + data.len() as u64)).into(),
                );
            }
            data.to_vec()
        } else {
            match inflate_capped(data, claimed(&e) as usize, cap) {
                Ok(v) => v,
                Err(Capped::Over) => {
                    return Err(
                        LimitExceeded::bytes(total_claim.max(MAX_DECOMPRESSED_BYTES + 1)).into(),
                    );
                }
                Err(Capped::Err(m)) => return Err(m.into()),
            }
        };
        spent += got.len() as u64;
        out.insert(e.name, got);
    }
    Ok(out)
}

// ================================================================================================
// Project mode (plugins/cli/project-zip.js): a project is edited, not re-exported.
// ================================================================================================

/// The error `DataView` throws for a read past the end.
const OUTSIDE: &str = "Offset is outside the bounds of the DataView";

fn rd16(b: &[u8], o: usize) -> Result<u16, String> {
    b.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| OUTSIDE.to_string())
}

fn rd32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| OUTSIDE.to_string())
}

/// One entry of a [`ProjectZip`] (project-zip.js:46-47): `{ name, offset, start, size, length,
/// method, crc, record, end }`. `offset` is the local header, `start` the compressed payload,
/// `size` its compressed length, `length` the inflated one, `record` the central-directory
/// record, `end` where the next entry (or the central directory) begins.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEntry {
    pub name: String,
    pub offset: usize,
    pub start: usize,
    pub size: usize,
    pub length: usize,
    pub method: u16,
    pub crc: u32,
    pub record: Vec<u8>,
    pub end: usize,
}

/// A project archive: each unchanged ZIP entry's local header and compressed payload are
/// retained, and only the members needed for analysis are inflated. Port of `ProjectZip`
/// (plugins/cli/project-zip.js:9-98).
#[derive(Clone, Debug)]
pub struct ProjectZip {
    pub bytes: Vec<u8>,
    /// by name, in central-directory order
    pub entries: IndexMap<String, ProjectEntry>,
    /// the offset of the central directory
    pub central: usize,
    /// the archive comment
    pub comment: Vec<u8>,
}

/// `node:zlib` `inflateRaw`: unlike the lenient `DecompressionStream` reader above, a truncated
/// or invalid stream is an error (Node's messages for the two commonest are used).
fn inflate_raw_strict(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut out: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut input = bytes;
    loop {
        let r = inflate(&mut state, input, &mut buf, MZFlush::None);
        out.extend_from_slice(&buf[..r.bytes_written]);
        input = &input[r.bytes_consumed..];
        match r.status {
            Ok(MZStatus::StreamEnd) => return Ok(out),
            Ok(_) => {
                if r.bytes_consumed == 0 && r.bytes_written == 0 {
                    return Err("unexpected end of file".to_string());
                }
            }
            Err(MZError::Buf) => return Err("unexpected end of file".to_string()),
            Err(_) => return Err("invalid data".to_string()),
        }
    }
}

impl ProjectZip {
    /// `new ProjectZip(bytes)` (project-zip.js:10-57).
    pub fn new(bytes: &[u8]) -> Result<ProjectZip, String> {
        let len = bytes.len();
        // the end-of-central-directory record: the last one whose comment length reaches the end
        let mut end: Option<usize> = None;
        if len >= 22 {
            let mut i = len - 22;
            let floor = len.saturating_sub(65557);
            loop {
                if rd32(bytes, i)? == 0x06054b50 && i + 22 + rd16(bytes, i + 20)? as usize == len {
                    end = Some(i);
                    break;
                }
                if i == floor {
                    break;
                }
                i -= 1;
            }
        }
        let Some(end) = end else {
            return Err("not a ZIP project".to_string());
        };
        let count = rd16(bytes, end + 10)? as usize;
        let mut at = rd32(bytes, end + 16)? as usize;
        if rd16(bytes, end + 4)? != 0
            || rd16(bytes, end + 6)? != 0
            || rd16(bytes, end + 8)? as usize != count
            || count == 65535
            || at == 0xffffffff
            || (end >= 20 && rd32(bytes, end - 20)? == 0x07064b50)
        {
            return Err(
                "--preserve-project does not support multi-disk or ZIP64 archives".to_string(),
            );
        }
        let central = at;
        let comment = bytes[end + 22..].to_vec();
        let directory_end = at + rd32(bytes, end + 12)? as usize;
        if directory_end != end {
            return Err("invalid ZIP directory".to_string());
        }
        let mut entries: IndexMap<String, ProjectEntry> = IndexMap::new();
        for _ in 0..count {
            if at + 46 > end || rd32(bytes, at)? != 0x02014b50 {
                return Err("invalid ZIP entry".to_string());
            }
            let name_len = rd16(bytes, at + 28)? as usize;
            let extra = rd16(bytes, at + 30)? as usize;
            let comment_len = rd16(bytes, at + 32)? as usize;
            let next = at + 46 + name_len + extra + comment_len;
            if next > end {
                return Err("truncated ZIP entry".to_string());
            }
            let name = decode(&bytes[at + 46..at + 46 + name_len]);
            let offset = rd32(bytes, at + 42)? as usize;
            let size = rd32(bytes, at + 20)? as usize;
            let length = rd32(bytes, at + 24)? as usize;
            let method = rd16(bytes, at + 10)?;
            let flags = rd16(bytes, at + 8)?;
            if [offset, size, length].contains(&0xffffffff) {
                return Err("ZIP64 entries are not supported in project mode".to_string());
            }
            if flags & 1 != 0 || ![0, 8].contains(&method) {
                return Err(format!("unsupported ZIP encoding: {name}"));
            }
            if rd16(bytes, at + 34)? != 0
                || offset + 30 > central
                || rd32(bytes, offset)? != 0x04034b50
            {
                return Err(format!("invalid ZIP member: {name}"));
            }
            let local_name = rd16(bytes, offset + 26)? as usize;
            let local_extra = rd16(bytes, offset + 28)? as usize;
            let start = offset + 30 + local_name + local_extra;
            let local = bytes
                .get(offset + 30..offset + 30 + local_name)
                .unwrap_or(&[]);
            if start + size > central || decode(local) != name {
                return Err(format!("invalid ZIP data: {name}"));
            }
            if entries.contains_key(&name)
                || name.starts_with('/')
                || name.contains('\\')
                || name.split('/').any(|p| p == ".." || p == ".")
            {
                return Err(format!("ambiguous ZIP path: {name}"));
            }
            let crc = rd32(bytes, at + 16)?;
            entries.insert(
                name.clone(),
                ProjectEntry {
                    name,
                    offset,
                    start,
                    size,
                    length,
                    method,
                    crc,
                    record: bytes[at..next].to_vec(),
                    end: 0,
                },
            );
            at = next;
        }
        if at != end {
            return Err("invalid ZIP directory length".to_string());
        }
        // `sorted.sort((a, b) => a.offset - b.offset)` (stable), then each entry ends where the
        // next begins
        let mut order: Vec<String> = entries.keys().cloned().collect();
        order.sort_by_key(|n| entries[n].offset);
        for (i, n) in order.iter().enumerate() {
            let e_end = order.get(i + 1).map_or(central, |m| entries[m].offset);
            let e = entries.get_mut(n).unwrap();
            e.end = e_end;
            if e.start + e.size > e.end {
                return Err(format!("overlapping ZIP entries: {}", e.name));
            }
        }
        Ok(ProjectZip {
            bytes: bytes.to_vec(),
            entries,
            central,
            comment,
        })
    }

    /// `read(name)` (project-zip.js:59-70): the member inflated and checked against its
    /// recorded length and CRC.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let e = self
            .entries
            .get(name)
            .ok_or_else(|| format!("missing project member: {name}"))?;
        let raw = &self.bytes[e.start..e.start + e.size];
        // node:zlib inflateRaw (both Node and Deno provide it)
        let data = if e.method == 0 {
            raw.to_vec()
        } else {
            inflate_raw_strict(raw)?
        };
        if data.len() != e.length || crc32(&data) != e.crc {
            return Err(format!("corrupt project member: {name}"));
        }
        Ok(data)
    }

    /// `text(name)` (project-zip.js:72): `new TextDecoder().decode(..)`.
    pub fn text(&self, name: &str) -> Result<String, String> {
        Ok(decode(&self.read(name)?))
    }

    /// `patch(changes)` (project-zip.js:74-96): the archive with `changes` applied (`None` deletes
    /// a member, `Some` replaces or adds one, as `zipStore` writes it). Every entry not named in
    /// `changes` is copied byte for byte: local header, compressed payload and central record
    /// (only its local-header offset is rewritten), in the original order; the changed members
    /// follow, in `changes` order; the archive comment is kept.
    pub fn patch(&self, changes: &[(String, Option<ZipData>)]) -> Result<Vec<u8>, String> {
        let files: Vec<ZipFile> = changes
            .iter()
            .filter_map(|(n, d)| {
                d.clone().map(|data| ZipFile {
                    name: n.clone(),
                    data,
                })
            })
            .collect();
        let added = ProjectZip::new(&zip_store(&files))?;
        let mut locals: Vec<Vec<u8>> = Vec::new();
        let mut records: Vec<Vec<u8>> = Vec::new();
        let mut offset = 0usize;
        let mut append = |zip: &'_ ProjectZip, e: &ProjectEntry| {
            let raw_start = e.offset;
            let raw_end = e.end;
            let mut record = e.record.clone();
            // `setUint32(42, offset, true)`
            record[42..46].copy_from_slice(&(offset as u32).to_le_bytes());
            locals.push(zip.bytes[raw_start..raw_end].to_vec());
            records.push(record);
            offset += raw_end - raw_start;
        };
        for e in self.entries.values() {
            if !changes.iter().any(|(n, _)| *n == e.name) {
                append(self, e);
            }
        }
        for e in added.entries.values() {
            append(&added, e);
        }
        let size: usize = records.iter().map(Vec::len).sum();
        if offset + size >= 0xffffffff || records.len() >= 65535 {
            return Err("output would require ZIP64".to_string());
        }
        let mut end = vec![0u8; 22 + self.comment.len()];
        end[0..4].copy_from_slice(&0x06054b50u32.to_le_bytes());
        end[8..10].copy_from_slice(&(records.len() as u16).to_le_bytes());
        end[10..12].copy_from_slice(&(records.len() as u16).to_le_bytes());
        end[12..16].copy_from_slice(&(size as u32).to_le_bytes());
        end[16..20].copy_from_slice(&(offset as u32).to_le_bytes());
        end[20..22].copy_from_slice(&(self.comment.len() as u16).to_le_bytes());
        end[22..].copy_from_slice(&self.comment);
        let mut out: Vec<u8> = Vec::new();
        for l in &locals {
            out.extend_from_slice(l);
        }
        for r in &records {
            out.extend_from_slice(r);
        }
        out.extend_from_slice(&end);
        Ok(out)
    }
}

#[cfg(test)]
mod inflate_tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    fn data(r: &mut Lcg, len: usize) -> Vec<u8> {
        // runs, a small alphabet and noise: stored, fixed and dynamic blocks all come up
        (0..len)
            .map(|i| match (r.next() % 4, i % 97) {
                (0, _) => r.next() as u8,
                (_, 0..=40) => b"<vertex x=\"1.5\" y=\"2\"/>"[i % 23],
                _ => (r.next() % 5) as u8 + b'a',
            })
            .collect()
    }

    /// The zlib-rs path agrees with the miniz_oxide definition on good streams, and every bad
    /// one (truncated, corrupted, trailing bytes) gives the same result because it is re-run there.
    #[test]
    fn fast_inflate_equals_miniz() {
        let mut r = Lcg(99);
        for case in 0..120 {
            let len = [0usize, 1, 10, 1000, 70_000, 400_000][case % 6] + (r.next() % 50) as usize;
            let raw = data(&mut r, len);
            let level = [0u8, 1, 6, 9][case % 4];
            let packed = miniz_oxide::deflate::compress_to_vec(&raw, level);
            let hint = [raw.len(), 0, 5, usize::MAX, raw.len() * 2][case % 5];
            let want = inflate_raw_miniz(&packed);
            assert_eq!(want.as_ref().ok(), Some(&raw), "case {case}");
            assert_eq!(inflate_raw(&packed, hint), want, "case {case}");

            let mut trailing = packed.clone();
            trailing.extend_from_slice(b"junk");
            assert_eq!(
                inflate_raw(&trailing, hint),
                inflate_raw_miniz(&trailing),
                "trailing, case {case}"
            );
            for _ in 0..6 {
                let cut = (r.next() as usize) % (packed.len() + 1);
                assert_eq!(
                    inflate_raw(&packed[..cut], hint),
                    inflate_raw_miniz(&packed[..cut]),
                    "cut {cut}, case {case}"
                );
                let mut bad = packed.clone();
                if !bad.is_empty() {
                    let at = (r.next() as usize) % bad.len();
                    bad[at] ^= 1 << (r.next() % 8);
                }
                assert_eq!(
                    inflate_raw(&bad, hint),
                    inflate_raw_miniz(&bad),
                    "flipped, case {case}"
                );
            }
        }
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    fn deflate_zeros(n: usize) -> Vec<u8> {
        use flate2::{Compress, Compression, FlushCompress};
        let mut c = Compress::new(Compression::fast(), false);
        let mut out = Vec::with_capacity(n / 500 + 64);
        let zeros = vec![0u8; 1 << 16];
        let mut left = n;
        while left > 0 {
            let k = left.min(zeros.len());
            let before = c.total_in();
            out.reserve(4096);
            c.compress_vec(&zeros[..k], &mut out, FlushCompress::None)
                .unwrap();
            left -= (c.total_in() - before) as usize;
        }
        loop {
            out.reserve(4096);
            let st = c
                .compress_vec(&[], &mut out, FlushCompress::Finish)
                .unwrap();
            if st == flate2::Status::StreamEnd {
                break;
            }
        }
        out
    }

    /// A stream that lies about its size (or has none): the running budget stops it, whichever
    /// inflater would have taken it, with no more than the cap and one buffer produced.
    #[test]
    fn a_stream_past_its_cap_stops_early() {
        let packed = deflate_zeros(4 << 20);
        assert!(packed.len() < 1 << 20, "{}", packed.len());
        assert!(matches!(
            inflate_capped(&packed, 10, 1 << 20),
            Err(Capped::Over)
        ));
        assert!(matches!(
            inflate_miniz_capped(&packed, 1 << 20),
            Err(Capped::Over)
        ));
        // under the cap, the same bytes
        let ok = inflate_capped(&packed, 0, 8 << 20).ok().unwrap();
        assert_eq!(ok.len(), 4 << 20);
    }
}
