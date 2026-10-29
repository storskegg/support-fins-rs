// Dev helper: deno run -A --config tools/i18n/deno.json tools/i18n/lookup.ts "string" ...  -- is it a web key, translated?
import { loadWeb, webLookup, norm, argForm } from "./web.ts";
const web = await loadWeb();
for (const q of Deno.args) {
  const hit = webLookup(web.known, q);
  const tr = Object.entries(web.dicts).filter(([, d]) => Object.keys(d).some((k) => argForm(norm(k)) === argForm(norm(q)))).map(([l]) => l);
  console.log(JSON.stringify(q), "->", hit ? JSON.stringify(hit) : "NO WEB KEY", tr.length ? `translated: ${tr.join(",")}` : "");
}
