// The Bambu-style project fixture of plugins/cli/tests/project.test.js (lines 10-117 there, copied
// verbatim apart from the imports): a two-plate project with compressed members, a Unicode member name,
// opaque binary content and an archive comment. `parity/cli/cli_parity.js` builds its project inputs
// with it, so the Rust project mode is run on the very files the JS tests use.
import { deflateRawSync } from "node:zlib";
import { UPSTREAM_ROOT } from "../oracle/lib/pin.js";

const { ProjectZip } = await import(`${UPSTREAM_ROOT}/plugins/cli/project-zip.js`);
const { meshXML } = await import(`${UPSTREAM_ROOT}/web/threemf.js`);
const { zipStore } = await import(`${UPSTREAM_ROOT}/web/zip.js`);
const { block } = await import(`${UPSTREAM_ROOT}/tests/_util.js`);

const ROOT = '3D/3dmodel.model', CONFIG = 'Metadata/model_settings.config';
const SETTINGS = 'Metadata/project_settings.config', SUPPORTS = '3D/SupportFins/supports.model';
const CORE = 'http://schemas.microsoft.com/3dmanufacturing/core/2015/02';
const PROD = 'http://schemas.microsoft.com/3dmanufacturing/production/2015/06';
const RELS = 'http://schemas.openxmlformats.org/package/2006/relationships';
const bytes = (s) => new TextEncoder().encode(s);
const triples = (flat) => Array.from({ length: flat.length / 3 }, (_, i) => Array.from(flat.slice(i * 3, i * 3 + 3)));
const equal = (a, b, label) => assert(a.length === b.length && a.every((v, i) => v === b[i]), label);

// Real compressed entries (also a Unicode name, opaque binary content and a ZIP
// comment), so a test cannot pass by rebuilding everything with zipStore.
async function compressed(files) {
  const zip = new ProjectZip(new Uint8Array(await zipStore(files).arrayBuffer()));
  const chunks = [], records = [];
  let offset = 0;
  for (const e of zip.entries.values()) {
    const header = zip.bytes.slice(e.offset, e.start), raw = deflateRawSync(await zip.read(e.name));
    const h = new DataView(header.buffer);
    h.setUint16(8, 8, true); h.setUint32(18, raw.length, true);
    const record = e.record.slice(), r = new DataView(record.buffer);
    r.setUint16(10, 8, true); r.setUint32(20, raw.length, true); r.setUint32(42, offset, true);
    chunks.push(header, raw); records.push(record); offset += header.length + raw.length;
  }
  const comment = bytes('test archive comment'), end = new Uint8Array(22 + comment.length), v = new DataView(end.buffer);
  v.setUint32(0, 0x06054b50, true); v.setUint16(8, records.length, true); v.setUint16(10, records.length, true);
  v.setUint32(12, records.reduce((n, r) => n + r.length, 0), true); v.setUint32(16, offset, true);
  v.setUint16(20, comment.length, true); end.set(comment, 22);
  return new Uint8Array(await new Blob([...chunks, ...records, end]).arrayBuffer());
}

// `negative` deliberately reclassifies the only body (a malformed empty object).
// `cutters` adds real negative components alongside the printable body.
export async function fixture({ duplicate = false, negative = false, variable = null, layer = '0.16', printable = true, mirror = false,
  app = 'BambuStudio-02.04.00.70', sequence = 'by layer', plateSequence = null,
  plateIDs = [1, 2], emptyPlates = [], together = false, sameName = false, steep = false, nozzle = ['0.4'], positions = null,
  cutters = [], upright = false } = {}) {
  const cube = triples(positions ?? block(0, 20, 0, 20, 0, 20));
  const q = upright ? 1 : steep ? 0.5 : Math.SQRT1_2, s = upright ? 0 : steep ? Math.sqrt(3) / 2 : q;
  const resource = (id, file) => `<object id="${id}" type="model" p:UUID="original-${id}"><components>`
    + `<component objectid="1" p:path="/3D/Objects/${file}.model" transform="1 0 0 0 1 0 0 0 1 0 0 0"/>`
    + (id === 10 ? cutters.map((c, i) => Array.from({ length: c.repeat ?? 1 }, () =>
      `<component objectid="${i + 2}" p:path="/3D/Objects/cutter${i}.model" transform="${c.transform ?? '1 0 0 0 1 0 0 0 1 0 0 0'}"/>`).join('')).join('') : '')
    + '</components></object>';
  const model = (paint) => `<model unit="millimeter" xmlns="${CORE}"><resources><object id="1" type="model">`
    + meshXML(cube, (i) => i % 3 ? ` paint_color="${paint}"` : '') + '</object></resources></model>';
  const obj = (id, slot) => `<object id="${id}"><metadata key="name" value="Part ${sameName ? 10 : id}"/>`
    + `<metadata key="extruder" value="${slot}"/><metadata key="enable_support" value="1"/>`
    + `<part id="1" subtype="${negative && id === 10 ? 'negative_part' : 'normal_part'}">`
    + '<metadata key="name" value="painted body"/><metadata key="enable_support" value="1"/></part>'
    + (id === 10 ? cutters.map((_, i) => `<part id="${i + 2}" subtype="negative_part"><metadata key="name" value="cutter ${i}"/>`
      + '<metadata key="matrix" value="original cutter metadata"/></part>').join('') : '') + '</object>';
  const instance = (id, n) => `<model_instance><metadata key="object_id" value="${id}"/><metadata key="instance_id" value="${duplicate && n === 2 ? 1 : 0}"/>`
    + '<metadata key="identify_id" value="99"/></model_instance>';
  const plate = (n, instances) => `<plate><metadata key="plater_id" value="${plateIDs[n - 1]}"/><metadata key="locked" value="true"/>`
    + (plateSequence && n === 1 ? `<metadata key="print_sequence" value="${plateSequence}"/>` : '')
    + `<metadata key="gcode_file" value="Metadata/plate_${n}.gcode"/>`
    + `<metadata key="thumbnail_file" value="Metadata/plate_${n}.png"/>`
    + instances + '</plate>';
  const second = instance(duplicate ? 10 : 20, 2);
  const files = [
    { name: ROOT, data: `<model xmlns="${CORE}" xmlns:p="${PROD}" unit="millimeter" requiredextensions="p">`
      + `<metadata name="Application">${app}</metadata><resources>`
      + resource(10, 'a') + resource(20, 'b') + '</resources><build>'
      + `<item objectid="10" printable="1" transform="${mirror ? -1 : 1} 0 0 0 ${q} ${s} 0 ${-s} ${q} 30 40 0"/>`
      + `<item objectid="${duplicate ? 10 : 20}" printable="${printable ? 1 : 0}" transform="1 0 0 0 ${q} ${s} 0 ${-s} ${q} 400 50 0"/>`
      + '</build></model>' },
    { name: CONFIG, data: `<config>${obj(10, 1)}${obj(20, 2)}`
      + plate(1, instance(10, 1) + (together ? second : '')) + plate(2, together ? '' : second)
      + emptyPlates.map((id) => `<plate><metadata key="plater_id" value="${id}"/></plate>`).join('') + '<assemble/></config>' },
    { name: SETTINGS, data: JSON.stringify({ layer_height: layer, initial_layer_print_height: '0.3', nozzle_diameter: nozzle,
      filament_colour: ['#FFFFFF', '#123456'], filament_type: ['PETG', 'PETG'], enable_support: '1', print_sequence: sequence }) },
    { name: '3D/Objects/a.model', data: model('4') },
    { name: '3D/Objects/b.model', data: model('8') },
    { name: '_rels/.rels', data: `<Relationships xmlns="${RELS}"><Relationship Id="root" Target="/${ROOT}" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>` },
    { name: '3D/_rels/3dmodel.model.rels', data: `<Relationships xmlns="${RELS}"><Relationship Id="a" Target="/3D/Objects/a.model" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>` },
    { name: '[Content_Types].xml', data: '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/Metadata/plate_1.gcode" ContentType="text/x.gcode"/></Types>' },
    { name: 'Metadata/plate_1.gcode', data: 'obsolete toolpath 1' },
    { name: 'Metadata/plate_2.gcode', data: 'obsolete toolpath 2' },
    { name: 'Metadata/plate_1.png', data: new Uint8Array([1, 3, 5]) },
    { name: 'Metadata/plate_2.png', data: new Uint8Array([2, 4, 6]) },
    { name: 'Metadata/custom_gcode_per_layer.xml', data: '<custom_gcodes><code type="pause" print_z="5"/></custom_gcodes>' },
    { name: 'Metadata/slice_info.config', data: '<config/>' },
    { name: 'Auxiliaries/私有.bin', data: new Uint8Array([0, 255, 1, 128]) },
  ];
  cutters.forEach((c, i) => {
    if (c.positions != null) files.push({ name: `3D/Objects/cutter${i}.model`,
      data: `<model unit="millimeter" xmlns="${CORE}"><resources><object id="${i + 2}" type="model">`
        + meshXML(triples(c.positions)) + '</object></resources></model>' });
  });
  // variable: the object_id a profile line names, by object order (1 = the first object)
  if (variable) files.push({ name: 'Metadata/layer_heights_profile.txt', data: `object_id=${variable}|0;0.2;5;0.1;20;0.1;\n` });
  return compressed(files);
}

