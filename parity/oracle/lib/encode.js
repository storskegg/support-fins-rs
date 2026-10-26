// The corpus encoder: the only writer of parity corpus files. Format: docs/PARITY.md
// ("Case format" and "Value encoding"). Each branch of encode() names the table row it implements.

const CORPUS = new URL("../../corpus/", import.meta.url);

const LITTLE_ENDIAN = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;

function b64(bytes) {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    s += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return btoa(s);
}

// Typed arrays are encoded from their exact bytes (NaN payloads included), little-endian.
function typedBytes(v) {
  if (!LITTLE_ENDIAN) throw new Error("encode: big-endian host unsupported");
  return new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
}

/**
 * A standard reference (docs/PARITY.md, "Standard references in captured args"): `payload` is an
 * already-encoded JSON tree such as `{"$topo": "cube"}`. encode() writes it as-is, since its
 * `$topo` key must not be escaped to `$$topo` like a plain object's `$` key.
 */
export class Ref {
  constructor(payload) {
    this.payload = payload;
  }
}

/** Encodes a JS value to a JSON-safe tree. */
export function encode(value) {
  if (value instanceof Ref) return value.payload;
  // Row: string, boolean, null -> JSON as-is
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  // Row: undefined -> {"$u": 1}
  if (value === undefined) return { $u: 1 };
  if (typeof value === "number") {
    // Row: -0, NaN, Infinity, -Infinity -> {"$n": "..."}
    if (Object.is(value, -0)) return { $n: "-0" };
    if (Number.isNaN(value)) return { $n: "NaN" };
    if (value === Infinity) return { $n: "Infinity" };
    if (value === -Infinity) return { $n: "-Infinity" };
    // Row: finite number, not -0 -> JSON number (JSON.stringify writes it, shortest round-trip)
    return value;
  }
  // Rows: Float64Array / Float32Array / Int32Array / Uint32Array / Uint8Array
  if (value instanceof Float64Array) return { $f64: b64(typedBytes(value)) };
  if (value instanceof Float32Array) return { $f32: b64(typedBytes(value)) };
  if (value instanceof Int32Array) return { $i32: b64(typedBytes(value)) };
  if (value instanceof Uint32Array) return { $u32: b64(typedBytes(value)) };
  if (value instanceof Uint8Array) return { $u8: b64(typedBytes(value)) };
  // Row: Array -> JSON array of encoded values
  if (Array.isArray(value)) {
    const out = new Array(value.length);
    for (let i = 0; i < value.length; i++) out[i] = encode(value[i]);
    return out;
  }
  // Row: Map -> {"$map": [[k, v], ...]} in insertion order
  if (value instanceof Map) {
    return { $map: Array.from(value, ([k, v]) => [encode(k), encode(v)]) };
  }
  // Row: Set -> {"$set": [v, ...]} in insertion order
  if (value instanceof Set) return { $set: Array.from(value, encode) };
  // Row: plain object -> JSON object, key order preserved, a key starting with `$` escaped as `$$`
  if (typeof value === "object") {
    const proto = Object.getPrototypeOf(value);
    if (proto !== Object.prototype && proto !== null) {
      throw new Error(`encode: unsupported object ${Object.prototype.toString.call(value)}`);
    }
    const out = {};
    for (const key of Object.keys(value)) {
      out[key.startsWith("$") ? "$" + key : key] = encode(value[key]);
    }
    return out;
  }
  throw new Error(`encode: unsupported value of type ${typeof value}`);
}

/**
 * Writes parity/corpus/<module>/<fn>.jsonl. `cases` is [{id, args, out} | {id, args, throws}].
 * One encoded case per line, `\n` line ends, trailing newline. Throws on a duplicate id.
 */
export function writeCases(module, fn, cases, { encoded = false } = {}) {
  const seen = new Set();
  const lines = [];
  for (const c of cases) {
    if (typeof c.id !== "string" || c.id === "") throw new Error(`writeCases ${module}/${fn}: case without id`);
    if (seen.has(c.id)) throw new Error(`writeCases ${module}/${fn}: duplicate id '${c.id}'`);
    seen.add(c.id);
    if (!Array.isArray(c.args)) throw new Error(`writeCases ${module}/${fn}: '${c.id}' args is not an array`);
    const hasThrows = "throws" in c && c.throws !== undefined;
    const hasOut = "out" in c;
    if (hasThrows === hasOut) {
      throw new Error(`writeCases ${module}/${fn}: '${c.id}' needs exactly one of out / throws`);
    }
    const enc = encoded ? (x) => x : encode; // `encoded`: values already went through encode()
    const line = { id: c.id, args: c.args.map(enc) };
    if (hasThrows) line.throws = String(c.throws);
    else line.out = enc(c.out);
    lines.push(JSON.stringify(line) + "\n");
  }
  const dir = new URL(`${module}/`, CORPUS);
  Deno.mkdirSync(dir, { recursive: true });
  const bytes = new TextEncoder().encode(lines.join(""));
  const plain = new URL(`${fn}.jsonl`, dir);
  const packed = new URL(`${fn}.jsonl.zst`, dir);
  // Over 5 MB the file is zstd-compressed (git LFS tracks *.zst). Only one form exists at a time.
  if (bytes.length > ZST_OVER) {
    writeZstd(packed, bytes);
    rmIfExists(plain);
  } else {
    Deno.writeFileSync(plain, bytes);
    rmIfExists(packed);
  }
}

const ZST_OVER = 5 * 1024 * 1024;

function rmIfExists(path) {
  try {
    Deno.removeSync(path);
  } catch (e) {
    if (!(e instanceof Deno.errors.NotFound)) throw e;
  }
}

// `zstd -19` is deterministic for the same input and zstd version.
function writeZstd(dest, bytes) {
  const tmp = Deno.makeTempFileSync();
  try {
    Deno.writeFileSync(tmp, bytes);
    const o = new Deno.Command("zstd", {
      args: ["-q", "-19", "-f", tmp, "-o", dest.pathname],
      stderr: "piped",
    }).outputSync();
    if (!o.success) throw new Error(`zstd failed: ${new TextDecoder().decode(o.stderr)}`);
  } finally {
    rmIfExists(tmp);
  }
}
