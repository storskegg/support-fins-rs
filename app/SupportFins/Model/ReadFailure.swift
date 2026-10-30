import Foundation

/// The text of a failed read: the web's alert, `Could not read ${file.name}:\n${err.message}` (`ui/io.js:407`). The
/// prefix is translated (`tools/i18n/native-only.json`; the web's alert is English only), the message is the engine's
/// own, English like the web's. Without a name (a part with no file) the message stands alone.
enum ReadFailure {
    static func text(of error: Error, reading name: String?, in language: L10n.Language? = nil) -> String {
        let message = error.localizedDescription
        guard let name else { return message }
        return JS.t("Could not read {name}:", ["name": name], in: language) + "\n" + message
    }
}
