# tools/i18n: the app's languages

The app follows the system language (en, de, zh-Hans, pt, ar, nl). Its strings are `app/SupportFins/Resources/Localizable.xcstrings`,
**generated** from the web's tables (`web/i18n/*.js` in the pinned JS checkout):

- `just i18n-gen` writes the catalog (run it at the upstream catch-up, and after editing `aliases.json` or `native-only.json`).
- `just i18n-catalog` (in `just gate`) fails when the file is not what the tables generate.
- `just i18n-check` (in `just gate`, after the app build) fails when a string the app shows is not a web string, or a web
  translation is missing from the catalog. It prints how many of the app's strings each language translates.

Keys are the web's English strings as Swift's string interpolation spells them: `{n}` is `%lld`, any other `{name}` is `%@`,
a `%` is doubled when there are arguments. SwiftUI literals (`Text("Draw")`) are looked up by SwiftUI; text built in code goes
through `JS.t(key, params)` / `JS.tn(singular, plural, n, params)`, which build the same key the web's `t()` / `tn()` look up. The web's plural
objects become plural variations; English gets its forms from the `tn()` pairs found in the web's JS (acorn). A `tn()` text that
never shows its count carries it as a hidden last `%lld` (its value is bracketed by U+2063, which `JS.tn` strips).

- `native-only.json`: strings the web has no key for (menus, undo names, sheets, errors), with the reason and translations
  (`like` borrows a web string's translations; `dev` marks debug-only text). These translations are the port's, not the web's.
- `aliases.json`: a spelling the app uses that differs from the web's key in quotes, dashes or a final period (the web's `t()` is
  lenient there, the catalog is not): `{ "app spelling": "web key" }`.
- `not-ui.json`: string literals in the app that are not shown (engine JSON keys, ids, debug output).
- `lookup.ts "text"`: is it a web key, and who translates it?

The web's tables are partial (German especially); the app shows English where the web does.
