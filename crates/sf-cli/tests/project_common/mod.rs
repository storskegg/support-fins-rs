//! What the ported `plugins/cli/tests/project*.test.js` share: the Bambu-style project fixture
//! (project.test.js:10-117), the in-memory command line with project I/O (`cli`, 119-129), and the
//! on-disk one for the resume tests (`diskProject`, 745-761).
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use sf_cli::cli::{Io, run};
use sf_cli::project_cache::{CacheRequest, CacheStore, Generated, open_project_cache};
use sf_cli::project_io;
use sf_engine::jsnum;
use sf_io::json::{Json, parse};
use sf_io::threemf::mesh_xml;
use sf_io::zip::{ProjectZip, ZipData, ZipFile, zip_store};
use sf_testkit::block;

pub const ROOT: &str = "3D/3dmodel.model";
pub const CONFIG: &str = "Metadata/model_settings.config";
pub const SETTINGS: &str = "Metadata/project_settings.config";
pub const SUPPORTS: &str = "3D/SupportFins/supports.model";
const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const PROD: &str = "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";
const RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

pub fn triples(flat: &[f64]) -> Vec<[f64; 3]> {
    flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

pub fn n(v: f64) -> String {
    jsnum::to_js_string(v)
}

/// One cutter of `fixture({ cutters })`: `{ positions, transform, repeat }`.
#[derive(Clone, Default)]
pub struct Cutter {
    /// `None` is `positions: null` (no member written)
    pub positions: Option<Vec<f64>>,
    pub transform: Option<String>,
    pub repeat: Option<usize>,
}

/// The options of `fixture({ ... })` with the JS defaults.
#[derive(Clone)]
pub struct Fx {
    pub duplicate: bool,
    pub negative: bool,
    pub variable: Option<f64>,
    pub layer: String,
    pub printable: bool,
    pub mirror: bool,
    pub app: String,
    pub sequence: String,
    pub plate_sequence: Option<String>,
    pub plate_ids: Vec<String>,
    pub empty_plates: Vec<String>,
    pub together: bool,
    pub same_name: bool,
    pub steep: bool,
    pub nozzle: Json,
    pub positions: Option<Vec<f64>>,
    pub cutters: Vec<Cutter>,
    pub upright: bool,
}

impl Default for Fx {
    fn default() -> Fx {
        Fx {
            duplicate: false,
            negative: false,
            variable: None,
            layer: "0.16".into(),
            printable: true,
            mirror: false,
            app: "BambuStudio-02.04.00.70".into(),
            sequence: "by layer".into(),
            plate_sequence: None,
            plate_ids: vec!["1".into(), "2".into()],
            empty_plates: vec![],
            together: false,
            same_name: false,
            steep: false,
            nozzle: Json::Arr(vec![Json::Str("0.4".into())]),
            positions: None,
            cutters: vec![],
            upright: false,
        }
    }
}

/// Real compressed entries (also a Unicode name, opaque binary content and a ZIP comment), so a
/// test cannot pass by rebuilding everything with zipStore (project.test.js:21-39).
pub fn compressed(files: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let zip_files: Vec<ZipFile> = files
        .into_iter()
        .map(|(name, data)| ZipFile {
            name,
            data: ZipData::Bytes(data),
        })
        .collect();
    let zip = ProjectZip::new(&zip_store(&zip_files)).unwrap();
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut records: Vec<Vec<u8>> = Vec::new();
    let mut offset = 0usize;
    for e in zip.entries.values() {
        let mut header = zip.bytes[e.offset..e.start].to_vec();
        let raw = miniz_oxide::deflate::compress_to_vec(&zip.read(&e.name).unwrap(), 6);
        header[8..10].copy_from_slice(&8u16.to_le_bytes());
        header[18..22].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        let mut record = e.record.clone();
        record[10..12].copy_from_slice(&8u16.to_le_bytes());
        record[20..24].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        record[42..46].copy_from_slice(&(offset as u32).to_le_bytes());
        offset += header.len() + raw.len();
        chunks.push(header);
        chunks.push(raw);
        records.push(record);
    }
    let comment = b"test archive comment";
    let mut end = vec![0u8; 22 + comment.len()];
    end[0..4].copy_from_slice(&0x06054b50u32.to_le_bytes());
    end[8..10].copy_from_slice(&(records.len() as u16).to_le_bytes());
    end[10..12].copy_from_slice(&(records.len() as u16).to_le_bytes());
    end[12..16]
        .copy_from_slice(&(records.iter().map(Vec::len).sum::<usize>() as u32).to_le_bytes());
    end[16..20].copy_from_slice(&(offset as u32).to_le_bytes());
    end[20..22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
    end[22..].copy_from_slice(comment);
    let mut out = Vec::new();
    for c in chunks {
        out.extend(c);
    }
    for r in records {
        out.extend(r);
    }
    out.extend(end);
    out
}

/// `fixture({ ... })` (project.test.js:44-117): a two-plate Bambu-style project.
pub fn fixture(o: &Fx) -> Vec<u8> {
    let cube = triples(
        &o.positions
            .clone()
            .unwrap_or_else(|| block(0.0, 20.0, 0.0, 20.0, 0.0, 20.0)),
    );
    let (q, s) = if o.upright {
        (1.0, 0.0)
    } else if o.steep {
        (0.5, 3f64.sqrt() / 2.0)
    } else {
        let q = std::f64::consts::FRAC_1_SQRT_2;
        (q, q)
    };
    let resource = |id: u32, file: &str| -> String {
        let mut cutter_components = String::new();
        if id == 10 {
            for (i, c) in o.cutters.iter().enumerate() {
                for _ in 0..c.repeat.unwrap_or(1) {
                    cutter_components += &format!(
                        "<component objectid=\"{}\" p:path=\"/3D/Objects/cutter{i}.model\" transform=\"{}\"/>",
                        i + 2,
                        c.transform
                            .clone()
                            .unwrap_or_else(|| "1 0 0 0 1 0 0 0 1 0 0 0".into())
                    );
                }
            }
        }
        format!(
            "<object id=\"{id}\" type=\"model\" p:UUID=\"original-{id}\"><components><component objectid=\"1\" p:path=\"/3D/Objects/{file}.model\" transform=\"1 0 0 0 1 0 0 0 1 0 0 0\"/>{cutter_components}</components></object>"
        )
    };
    let model = |paint: &str| -> String {
        let attrs = |i: usize| {
            if !i.is_multiple_of(3) {
                format!(" paint_color=\"{paint}\"")
            } else {
                String::new()
            }
        };
        format!(
            "<model unit=\"millimeter\" xmlns=\"{CORE}\"><resources><object id=\"1\" type=\"model\">{}</object></resources></model>",
            mesh_xml(&cube, Some(&attrs), None)
        )
    };
    let obj = |id: u32, slot: u32| -> String {
        let name_n = if o.same_name { 10 } else { id };
        let subtype = if o.negative && id == 10 {
            "negative_part"
        } else {
            "normal_part"
        };
        let mut cutter_parts = String::new();
        if id == 10 {
            for i in 0..o.cutters.len() {
                cutter_parts += &format!(
                    "<part id=\"{}\" subtype=\"negative_part\"><metadata key=\"name\" value=\"cutter {i}\"/><metadata key=\"matrix\" value=\"original cutter metadata\"/></part>",
                    i + 2
                );
            }
        }
        format!(
            "<object id=\"{id}\"><metadata key=\"name\" value=\"Part {name_n}\"/><metadata key=\"extruder\" value=\"{slot}\"/><metadata key=\"enable_support\" value=\"1\"/><part id=\"1\" subtype=\"{subtype}\"><metadata key=\"name\" value=\"painted body\"/><metadata key=\"enable_support\" value=\"1\"/></part>{cutter_parts}</object>"
        )
    };
    let instance = |id: u32, nth: u32| -> String {
        format!(
            "<model_instance><metadata key=\"object_id\" value=\"{id}\"/><metadata key=\"instance_id\" value=\"{}\"/><metadata key=\"identify_id\" value=\"99\"/></model_instance>",
            if o.duplicate && nth == 2 { 1 } else { 0 }
        )
    };
    let plate = |nth: usize, instances: &str| -> String {
        let seq = match (&o.plate_sequence, nth) {
            (Some(p), 1) => format!("<metadata key=\"print_sequence\" value=\"{p}\"/>"),
            _ => String::new(),
        };
        format!(
            "<plate><metadata key=\"plater_id\" value=\"{}\"/><metadata key=\"locked\" value=\"true\"/>{seq}<metadata key=\"gcode_file\" value=\"Metadata/plate_{nth}.gcode\"/><metadata key=\"thumbnail_file\" value=\"Metadata/plate_{nth}.png\"/>{instances}</plate>",
            o.plate_ids[nth - 1]
        )
    };
    let second = instance(if o.duplicate { 10 } else { 20 }, 2);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |name: &str, data: Vec<u8>| files.push((name.to_string(), data));
    add(
        ROOT,
        format!(
            "<model xmlns=\"{CORE}\" xmlns:p=\"{PROD}\" unit=\"millimeter\" requiredextensions=\"p\"><metadata name=\"Application\">{}</metadata><resources>{}{}</resources><build><item objectid=\"10\" printable=\"1\" transform=\"{} 0 0 0 {} {} 0 {} {} 30 40 0\"/><item objectid=\"{}\" printable=\"{}\" transform=\"1 0 0 0 {} {} 0 {} {} 400 50 0\"/></build></model>",
            o.app,
            resource(10, "a"),
            resource(20, "b"),
            if o.mirror { -1 } else { 1 },
            n(q),
            n(s),
            n(-s),
            n(q),
            if o.duplicate { 10 } else { 20 },
            if o.printable { 1 } else { 0 },
            n(q),
            n(s),
            n(-s),
            n(q)
        )
        .into_bytes(),
    );
    let empties: String = o
        .empty_plates
        .iter()
        .map(|id| format!("<plate><metadata key=\"plater_id\" value=\"{id}\"/></plate>"))
        .collect();
    add(
        CONFIG,
        format!(
            "<config>{}{}{}{}{empties}<assemble/></config>",
            obj(10, 1),
            obj(20, 2),
            plate(
                1,
                &(instance(10, 1) + if o.together { &second } else { "" })
            ),
            plate(2, if o.together { "" } else { &second })
        )
        .into_bytes(),
    );
    let settings = Json::Obj(vec![
        ("layer_height".into(), Json::Str(o.layer.clone())),
        ("initial_layer_print_height".into(), Json::Str("0.3".into())),
        ("nozzle_diameter".into(), o.nozzle.clone()),
        (
            "filament_colour".into(),
            Json::Arr(vec![
                Json::Str("#FFFFFF".into()),
                Json::Str("#123456".into()),
            ]),
        ),
        (
            "filament_type".into(),
            Json::Arr(vec![Json::Str("PETG".into()), Json::Str("PETG".into())]),
        ),
        ("enable_support".into(), Json::Str("1".into())),
        ("print_sequence".into(), Json::Str(o.sequence.clone())),
    ]);
    add(SETTINGS, settings.stringify().into_bytes());
    add("3D/Objects/a.model", model("4").into_bytes());
    add("3D/Objects/b.model", model("8").into_bytes());
    add(
        "_rels/.rels",
        format!("<Relationships xmlns=\"{RELS}\"><Relationship Id=\"root\" Target=\"/{ROOT}\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>").into_bytes(),
    );
    add(
        "3D/_rels/3dmodel.model.rels",
        format!("<Relationships xmlns=\"{RELS}\"><Relationship Id=\"a\" Target=\"/3D/Objects/a.model\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>").into_bytes(),
    );
    add(
        "[Content_Types].xml",
        b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Override PartName=\"/Metadata/plate_1.gcode\" ContentType=\"text/x.gcode\"/></Types>".to_vec(),
    );
    add("Metadata/plate_1.gcode", b"obsolete toolpath 1".to_vec());
    add("Metadata/plate_2.gcode", b"obsolete toolpath 2".to_vec());
    add("Metadata/plate_1.png", vec![1, 3, 5]);
    add("Metadata/plate_2.png", vec![2, 4, 6]);
    add(
        "Metadata/custom_gcode_per_layer.xml",
        b"<custom_gcodes><code type=\"pause\" print_z=\"5\"/></custom_gcodes>".to_vec(),
    );
    add("Metadata/slice_info.config", b"<config/>".to_vec());
    add("Auxiliaries/私有.bin", vec![0, 255, 1, 128]);
    for (i, c) in o.cutters.iter().enumerate() {
        if let Some(p) = &c.positions {
            add(
                &format!("3D/Objects/cutter{i}.model"),
                format!(
                    "<model unit=\"millimeter\" xmlns=\"{CORE}\"><resources><object id=\"{}\" type=\"model\">{}</object></resources></model>",
                    i + 2,
                    mesh_xml(&triples(p), None, None)
                )
                .into_bytes(),
            );
        }
    }
    // variable: the object_id a profile line names, by object order (1 = the first object)
    if let Some(v) = o.variable {
        add(
            "Metadata/layer_heights_profile.txt",
            format!("object_id={}|0;0.2;5;0.1;20;0.1;\n", n(v)).into_bytes(),
        );
    }
    compressed(files)
}

/// What `cli(..)` returns.
pub struct Run {
    pub code: i32,
    pub fs: BTreeMap<String, Vec<u8>>,
    pub out: Vec<String>,
    pub err: Vec<String>,
    /// `out.length ? JSON.parse(out[0]) : null`
    pub result: Option<Json>,
}

impl Run {
    pub fn r(&self) -> &Json {
        self.result.as_ref().expect("a JSON result on stdout")
    }
    pub fn err_text(&self) -> String {
        self.err.join("\n")
    }
    /// `JSON.parse(r.err.at(-1))`
    pub fn last_err_json(&self) -> Json {
        parse(self.err.last().expect("an error line")).expect("error line is JSON")
    }
}

/// The `overrides` of `cli(source, flags, additional, overrides)`.
#[derive(Clone, Default)]
pub struct Overrides {
    /// `checkProjectOutput: undefined, writeProject: undefined`
    pub no_project_io: bool,
    /// `write`, `checkProjectOutput` and `writeProject` that throw
    pub forbid_output: bool,
    /// `writeProject` throws this
    pub write_project_error: Option<String>,
    /// `read` throws this
    pub read_error: Option<String>,
}

pub struct MemIo {
    pub fs: RefCell<BTreeMap<String, Vec<u8>>>,
    pub out: RefCell<Vec<String>>,
    pub err: RefCell<Vec<String>>,
    pub ov: Overrides,
}

impl Io for MemIo {
    fn read(&self, p: &str) -> Result<Vec<u8>, String> {
        if let Some(e) = &self.ov.read_error {
            return Err(e.clone());
        }
        // `fs.get(p)`: an unknown file is `undefined`
        self.fs
            .borrow()
            .get(p)
            .cloned()
            .ok_or_else(|| format!("No such file: {p}"))
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<(), String> {
        if self.ov.forbid_output {
            return Err("check touched output".into());
        }
        self.fs.borrow_mut().insert(p.into(), b.to_vec());
        Ok(())
    }
    fn out(&self, l: &str) {
        self.out.borrow_mut().push(l.into());
    }
    fn err(&self, l: &str) {
        self.err.borrow_mut().push(l.into());
    }
    fn has_project_io(&self) -> bool {
        !self.ov.no_project_io
    }
    fn check_project_output(
        &self,
        input: &str,
        output: &str,
        overwrite: bool,
    ) -> Result<(), String> {
        if self.ov.forbid_output {
            return Err("check touched output".into());
        }
        if input == output || self.fs.borrow().contains_key(output) && !overwrite {
            return Err("unsafe output".into());
        }
        Ok(())
    }
    fn write_project(
        &self,
        _input: &str,
        output: &str,
        b: &[u8],
        _overwrite: bool,
    ) -> Result<(), String> {
        if self.ov.forbid_output {
            return Err("check touched output".into());
        }
        if let Some(e) = &self.ov.write_project_error {
            return Err(e.clone());
        }
        self.fs.borrow_mut().insert(output.into(), b.to_vec());
        Ok(())
    }
}

/// `run(argv, io)` over an in-memory file system.
pub fn run_raw(files: &[(&str, Vec<u8>)], argv: &[String], ov: Overrides) -> Run {
    let mut fs = BTreeMap::new();
    for (k, v) in files {
        fs.insert(k.to_string(), v.clone());
    }
    let io = MemIo {
        fs: RefCell::new(fs),
        out: RefCell::new(vec![]),
        err: RefCell::new(vec![]),
        ov,
    };
    let code = run(argv, &io);
    let out = io.out.take();
    let result = out.first().and_then(|l| parse(l));
    Run {
        code,
        fs: io.fs.take(),
        out,
        err: io.err.take(),
        result,
    }
}

/// `cli(source, flags, additional, overrides)` (project.test.js:119-129).
pub fn cli_with(
    source: &[u8],
    flags: &[&str],
    additional: &[(&str, Vec<u8>)],
    ov: Overrides,
) -> Run {
    let mut files: Vec<(&str, Vec<u8>)> = vec![("part.3mf", source.to_vec())];
    files.extend(additional.iter().cloned());
    let mut argv: Vec<String> = vec![
        "part.3mf".into(),
        "--preserve-project".into(),
        "--json".into(),
    ];
    argv.extend(flags.iter().map(|s| s.to_string()));
    run_raw(&files, &argv, ov)
}

pub fn cli(source: &[u8], flags: &[&str]) -> Run {
    cli_with(source, flags, &[], Overrides::default())
}

pub fn equal(a: &[u8], b: &[u8], label: &str) {
    assert!(a == b, "{label}");
}

/// `JSON` accessors for results.
pub fn j<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no key {k} in {}", v.stringify()))
}
pub fn jn(v: &Json, k: &str) -> f64 {
    match j(v, k) {
        Json::Num(n) => *n,
        o => panic!("{k}: {o:?}"),
    }
}
pub fn js(v: &Json, k: &str) -> String {
    match j(v, k) {
        Json::Str(s) => s.clone(),
        o => panic!("{k}: {o:?}"),
    }
}
pub fn ja<'a>(v: &'a Json, k: &str) -> &'a [Json] {
    match j(v, k) {
        Json::Arr(a) => a,
        o => panic!("{k}: {o:?}"),
    }
}
/// `r.result.objects.map((o) => o.number).join()`
pub fn numbers(r: &Run) -> String {
    ja(r.r(), "objects")
        .iter()
        .map(|o| n(jn(o, "number")))
        .collect::<Vec<_>>()
        .join(",")
}

/// `new ProjectZip(zip.patch(new Map([[name, content], ...])))`: the project with members replaced
/// (`Some`), deleted (`None`) or added.
pub fn patched(source: &[u8], changes: &[(&str, Option<Vec<u8>>)]) -> Vec<u8> {
    let zip = ProjectZip::new(source).unwrap();
    let ch: Vec<(String, Option<ZipData>)> = changes
        .iter()
        .map(|(n, d)| (n.to_string(), d.clone().map(ZipData::Bytes)))
        .collect();
    zip.patch(&ch).unwrap()
}

/// The text of a member after a `replace`.
pub fn member_text(source: &[u8], name: &str) -> String {
    ProjectZip::new(source).unwrap().text(name).unwrap()
}

// ---- the on-disk command line for the resume tests ---------------------------------------------

/// What a test hooks into the cache (`wrap`, project.test.js:735-744).
#[derive(Clone, Default)]
pub struct CacheHooks {
    pub load_fails_at: Option<usize>,
    pub save_fails_at: Option<usize>,
}

struct Wrapped {
    inner: Box<dyn CacheStore>,
    hooks: CacheHooks,
    hits: Rc<RefCell<Vec<usize>>>,
    saved: Rc<RefCell<Vec<usize>>>,
}

impl CacheStore for Wrapped {
    fn load(&self, number: usize, warn: &dyn Fn(&str)) -> Option<Generated> {
        if self.hooks.load_fails_at == Some(number) {
            // JS: the wrapper throws; the run stops with this message
            panic!("simulated interruption");
        }
        let r = self.inner.load(number, warn);
        if r.is_some() {
            self.hits.borrow_mut().push(number);
        }
        r
    }
    fn save(&mut self, number: usize, g: &Generated) -> Result<(), String> {
        if self.hooks.save_fails_at == Some(number) {
            return Err("checkpoint disk full".into());
        }
        self.inner.save(number, g)?;
        self.saved.borrow_mut().push(number);
        Ok(())
    }
    fn finish(&self, warn: &dyn Fn(&str)) {
        self.inner.finish(warn);
    }
}

pub struct DiskIo {
    pub out: RefCell<Vec<String>>,
    pub err: RefCell<Vec<String>>,
    pub hooks: CacheHooks,
    pub hits: Rc<RefCell<Vec<usize>>>,
    pub saved: Rc<RefCell<Vec<usize>>>,
    pub write_error: Option<String>,
}

impl Io for DiskIo {
    fn read(&self, p: &str) -> Result<Vec<u8>, String> {
        std::fs::read(p).map_err(|e| e.to_string())
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<(), String> {
        std::fs::write(p, b).map_err(|e| e.to_string())
    }
    fn out(&self, l: &str) {
        self.out.borrow_mut().push(l.into());
    }
    fn err(&self, l: &str) {
        self.err.borrow_mut().push(l.into());
    }
    fn has_project_io(&self) -> bool {
        true
    }
    fn check_project_output(&self, i: &str, o: &str, w: bool) -> Result<(), String> {
        project_io::check_project_output(i, o, w)
    }
    fn write_project(&self, i: &str, o: &str, b: &[u8], w: bool) -> Result<(), String> {
        if let Some(e) = &self.write_error {
            return Err(e.clone());
        }
        project_io::write_project(i, o, b, w)
    }
    fn has_project_cache(&self) -> bool {
        true
    }
    fn open_project_cache(
        &self,
        req: &CacheRequest,
        warn: &dyn Fn(&str),
    ) -> Result<Box<dyn CacheStore>, String> {
        let cache = open_project_cache(req, warn, &|| Ok("integration-test".to_string()))?;
        Ok(Box::new(Wrapped {
            inner: Box::new(cache),
            hooks: self.hooks.clone(),
            hits: self.hits.clone(),
            saved: self.saved.clone(),
        }))
    }
}

pub struct DiskRun {
    pub code: i32,
    pub out: Vec<String>,
    pub err: Vec<String>,
    pub hits: Vec<usize>,
    pub saved: Vec<usize>,
    pub result: Option<Json>,
}

impl DiskRun {
    pub fn r(&self) -> &Json {
        self.result.as_ref().expect("result")
    }
    pub fn err_text(&self) -> String {
        self.err.join("\n")
    }
}

/// A scratch directory with the project written to `part.3mf` (`diskProject`).
pub struct Disk {
    pub dir: std::path::PathBuf,
    pub input: String,
    pub source: Vec<u8>,
}

impl Drop for Disk {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn disk_project(o: &Fx) -> Disk {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "sf-cli-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let source = fixture(&Fx {
        steep: true,
        ..o.clone()
    });
    let input = dir.join("part.3mf").to_string_lossy().into_owned();
    std::fs::write(&input, &source).unwrap();
    Disk { dir, input, source }
}

impl Disk {
    pub fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }

    /// `call(flags, wrap, writer)`.
    pub fn call(&self, flags: &[&str], hooks: CacheHooks, write_error: Option<&str>) -> DiskRun {
        let io = DiskIo {
            out: RefCell::new(vec![]),
            err: RefCell::new(vec![]),
            hooks,
            hits: Rc::new(RefCell::new(vec![])),
            saved: Rc::new(RefCell::new(vec![])),
            write_error: write_error.map(str::to_string),
        };
        let mut argv: Vec<String> = vec![
            self.input.clone(),
            "--preserve-project".into(),
            "--json".into(),
        ];
        argv.extend(flags.iter().map(|s| s.to_string()));
        // a `throw` in a cache hook ends the run (code 1) as the JS does: here it is a panic
        let code =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&argv, &io))).unwrap_or(1);
        let out = io.out.take();
        let result = if code == 0 {
            out.first().and_then(|l| parse(l))
        } else {
            None
        };
        DiskRun {
            code,
            out,
            err: io.err.take(),
            hits: io.hits.take(),
            saved: io.saved.take(),
            result,
        }
    }
}

pub fn exists(p: &str) -> bool {
    std::path::Path::new(p).exists()
}
