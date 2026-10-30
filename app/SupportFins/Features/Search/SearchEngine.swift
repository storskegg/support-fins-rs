import Foundation

/// The settings search (`web/ui/search.js`, local issue 42): narrows the options panel to the settings whose name
/// matches, opens the folded sections they sit in, and puts the panel back as it was when cleared.
///
/// - A "row" is one setting (`SearchCatalog.rows`).
/// - Names first: every word typed must appear in the row's name (as shown, and the English it was translated
///   from). Only when no name matches does it fall back to the tooltips, so "gap" finds Support gap and Pad gap,
///   not every tooltip that mentions a gap.
/// - A match the current mode hides (Brace depth with sway off, a pad number outside Custom) stays hidden -- no
///   control the math ignores -- and the switch that reveals it is shown with it (`gate`). Turn it on and the match appears.
/// Layout only: it shows and hides rows, never changes a value.
struct SearchEngine {
    /// What one run of a query decided.
    struct Result: Equatable {
        /// Rows to show: the matches and the switches that reveal the ones a mode hides (the page's `.hit`).
        var show: Set<String> = []
        /// The blocks (sections, groups) that hold a row to show; the others are hidden (the page's `.nohit`).
        var blocks: Set<SearchBlock> = []
        /// Nothing to show: "No settings match".
        var none = false
    }

    /// The gates this query has shown, kept while the query stays the same (they stay to switch back off).
    private(set) var gated: Set<String> = []
    private var gatedFor = ""

    /// `fold`: lower case, accents removed.
    static func fold(_ s: String) -> String {
        s.lowercased().decomposedStringWithCanonicalMapping.unicodeScalars
            .filter { !(0x300...0x36F).contains($0.value) }.reduce(into: "") { $0.unicodeScalars.append($1) }
    }

    /// `localize` turns an English piece into the language on screen (the identity in English).
    var localize: (String) -> String = { JS.t($0) }

    /// `nameOf`: the name as shown plus the English it came from, folded.
    func name(of row: SearchEntry, _ ctx: SearchContext) -> String {
        let english = row.names(ctx)
        return Self.fold((english.map(localize) + english).joined(separator: " "))
    }

    /// `tipOf`: the tooltip as shown plus the English one.
    func tip(of row: SearchEntry, _ ctx: SearchContext) -> String {
        let t = row.tip(ctx)
        return Self.fold("\(localize(t.shown)) \(t.orig)")
    }

    /// `runSearch`: the query's result for this window, or nil when the box is empty (the panel as it was).
    mutating func run(query: String, _ ctx: SearchContext, rows: [SearchEntry] = SearchCatalog.rows) -> Result? {
        let words = Self.fold(query).split(whereSeparator: { $0.isWhitespace }).map(String.init)
        guard !words.isEmpty else {
            gated = []
            gatedFor = ""
            return nil
        }
        func has(_ text: String) -> Bool { words.allSatisfy { text.contains($0) } }
        var hits = rows.filter { has(name(of: $0, ctx)) }
        if hits.isEmpty { hits = rows.filter { has(tip(of: $0, ctx)) } }
        var show = Set(hits.map(\.id))
        if query != gatedFor {
            gated = []
            gatedFor = query
        }
        for r in hits where !r.shown(ctx) {
            if let g = r.gate(ctx) { gated.insert(g) }
        }
        show.formUnion(gated)   // a gate stays once it revealed a match, to switch back off

        // hide what holds none
        var blocks = Set<SearchBlock>()
        for id in show {
            guard let row = SearchCatalog.byID[id], row.shown(ctx) else { continue }
            blocks.formUnion(row.blocks)
        }
        return Result(show: show, blocks: blocks, none: !show.contains { SearchCatalog.byID[$0]?.shown(ctx) == true })
    }
}
