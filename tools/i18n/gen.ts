// Generates app/SupportFins/Resources/Localizable.xcstrings from the web's dictionaries (the JS checkout in UPSTREAM).
//
//   deno run -A tools/i18n/gen.ts           write the catalog
//   deno run -A tools/i18n/gen.ts --check   fail when the file on disk is not what the web's tables generate
//
// Re-run it at the upstream catch-up (`just i18n-gen`). Strings the web spells differently from the app go in
// tools/i18n/aliases.json (see parity.ts).
import { buildCatalog, serialize } from "./catalog.ts";

const here = new URL(".", import.meta.url).pathname;
const OUT = `${here}../../app/SupportFins/Resources/Localizable.xcstrings`;

export function readAliases(): Record<string, string> {
  return JSON.parse(Deno.readTextFileSync(`${here}aliases.json`));
}

if (import.meta.main) {
  const { catalog, report } = await buildCatalog(undefined, readAliases());
  const text = serialize(catalog);
  for (const [name, list] of Object.entries({ "translation uses a placeholder its key lacks": report.strays, "keys that norm alike with different values": report.normCollisions, "plural object on a key without {n}": report.missingN })) {
    if (list.length) console.warn(`i18n: ${name} (${list.length}):\n  ${list.slice(0, 20).join("\n  ")}`);
  }
  if (report.stringOnlyPairs.length) console.warn(`i18n: ${report.stringOnlyPairs.length} tn() pairs are plain strings in a language; English plural rule applies there`);
  if (Deno.args.includes("--check")) {
    let disk = "";
    try { disk = Deno.readTextFileSync(OUT); } catch { /* missing */ }
    if (disk !== text) {
      console.error("i18n: app/SupportFins/Resources/Localizable.xcstrings is out of date with the web's tables; run `just i18n-gen`");
      Deno.exit(1);
    }
    console.log(`i18n: catalog is current (${report.keys} keys)`);
  } else {
    Deno.mkdirSync(`${here}../../app/SupportFins/Resources`, { recursive: true });
    Deno.writeTextFileSync(OUT, text);
    console.log(`i18n: wrote ${report.keys} keys to app/SupportFins/Resources/Localizable.xcstrings`);
  }
}
