import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// The "Load anyway?" prompt of a file over a load limit (`FfiError.LimitExceeded`, docs/FFI.md "Load limits"): its
/// words in each language, Cancel loading nothing, Load anyway retrying with `allow_large`, and the note for a session
/// too deep to read. The inputs are generated small: a 12-triangle box 3 m wide, a 3MF whose fan-out is only counted,
/// a session nested past the JSON limit.
@MainActor
@Suite("Load limits")
struct LoadLimitTests {
    private func lang(_ code: String) throws -> L10n.Language { try #require(L10n.Language(code)) }

    // MARK: the words

    @Test("each kind says what the file needs and what the limit is, from the numbers in the error")
    func messagesInEnglish() throws {
        let en = try lang("en")
        let tri = LoadLimitPrompt(kind: .triangles, estimate: 140_000_000, limit: 20_000_000, name: nil)
        #expect(tri.message(in: en) == "This file expands to 140,000,000 triangles, which may need about 39 GB of memory.")
        let dim = LoadLimitPrompt(kind: .dimension, estimate: 7_600_000, limit: 2000, name: nil)
        #expect(dim.message(in: en) == "This model is 7.6 km across (the limit is 2 m).")
        let bytes = LoadLimitPrompt(kind: .bytes, estimate: 3.2 * 1_073_741_824, limit: 2_147_483_648, name: nil)
        #expect(bytes.message(in: en) == "This file unpacks to 3.2 GB (the limit is 2 GB).")
        // the limit is the error's, not a constant: a tuned limit reads as tuned
        #expect(LoadLimitPrompt(kind: .dimension, estimate: 9000, limit: 5000, name: nil).message(in: en) == "This model is 9 m across (the limit is 5 m).")
        #expect(LoadLimitPrompt(kind: .triangles, estimate: 3_000_000, limit: 1_000_000, name: nil).message(in: en).contains("about 840 MB"))
    }

    @Test("a small estimate of memory is said in whole megabytes, a mid one with a decimal")
    func memoryFigure() throws {
        let en = try #require(L10n.Language("en")).locale
        #expect(LoadLimitPrompt.memory(21_000_000 * 280, en) == "5.9 GB")
        #expect(LoadLimitPrompt.memory(140_000_000 * 280, en) == "39 GB")
        #expect(LoadLimitPrompt.memory(1_000_000 * 280, en) == "280 MB")
    }

    @Test("the title names the file", arguments: ["en", "de", "zh-Hans", "pt", "ar", "nl"])
    func titles(code: String) throws {
        let l = try lang(code)
        let named = LoadLimitPrompt(kind: .dimension, estimate: 3000, limit: 2000, name: "huge.stl").title(in: l)
        #expect(named.contains("huge.stl") && !named.contains("{"))
        let bare = LoadLimitPrompt(kind: .dimension, estimate: 3000, limit: 2000, name: nil).title(in: l)
        #expect(!bare.contains("{") && !bare.contains("huge"))
        if code != "en" { #expect(bare != "Load this file anyway?", "\(code) translates the title") }
    }

    @Test("every kind is translated, with its numbers filled in", arguments: ["de", "zh-Hans", "pt", "ar", "nl"])
    func messagesTranslated(code: String) throws {
        let l = try lang(code), en = try lang("en")
        for kind in [LimitKind.triangles, .bytes, .dimension] {
            let p = LoadLimitPrompt(kind: kind, estimate: 7_600_000, limit: 2000, name: nil)
            let text = p.message(in: l)
            #expect(text != p.message(in: en), "\(code) translates \(kind)")
            #expect(!text.contains("{") && !text.contains("}"), "\(code) \(kind): \(text)")
        }
        // the numbers survive in the language's own digits or ours, but the unit and a figure are always there
        #expect(LoadLimitPrompt(kind: .dimension, estimate: 7_600_000, limit: 2000, name: nil).message(in: l).contains("km"))
        #expect(JS.t("Load anyway", in: l) != "Load anyway", "\(code) translates the button")
        #expect(JS.t("The settings saved in this file couldn't be read; defaults are used.", in: l) != "The settings saved in this file couldn't be read; defaults are used.")
    }

    // MARK: the flow

    /// A box `edge` mm wide, as a binary STL.
    private func box(_ edge: Float, name: String) throws -> URL {
        URL(fileURLWithPath: try TestParts.write(TestParts.box(0, edge, 0, edge, 0, edge), name: name))
    }

    /// Starts the load and waits for the question; the load stays pending until it is answered.
    private func loadUntilAsked(_ model: ViewerModel, _ url: URL) async throws -> (load: Task<Void, Never>, prompt: LoadLimitPrompt) {
        let load = Task { await model.load(url: url) }
        #expect(await eventually { model.limitPrompt != nil }, "the load asks before it goes on")
        return (load, try #require(model.limitPrompt))
    }

    @Test("a part over 2 m asks, Cancel loads nothing, and Load anyway loads it with allow_large")
    func dimensionPrompt() async throws {
        let url = try box(3000, name: "limit-3m-box.stl")
        let model = ViewerModel()

        let (first, prompt) = try await loadUntilAsked(model, url)
        #expect(prompt.kind == .dimension && prompt.estimate == 3000 && prompt.limit == 2000)
        #expect(prompt.name == "limit-3m-box.stl")
        #expect(prompt.message(in: try lang("en")) == "This model is 3 m across (the limit is 2 m).")
        #expect(!model.hasPart && model.isLoading)
        model.answerLimitPrompt(false)
        await first.value
        #expect(model.limitPrompt == nil && !model.hasPart && model.session.parts.isEmpty && model.status == nil)

        let (second, again) = try await loadUntilAsked(model, url)
        #expect(again.kind == .dimension)
        model.answerLimitPrompt(true)
        await second.value
        #expect(model.limitPrompt == nil && model.hasPart && model.status == nil)
        #expect(model.session.parts.count == 1 && model.controller?.size == SIMD3<Float>(3000, 3000, 3000))
    }

    @Test("a part under the limit never asks")
    func smallPartLoads() async throws {
        let model = ViewerModel()
        await model.load(url: try box(40, name: "limit-40mm-box.stl"))
        #expect(model.limitPrompt == nil && model.hasPart)
    }

    @Test("a 3MF whose fan-out is over 20 million triangles asks before anything is expanded; Cancel keeps what is on show")
    func trianglePrompt() async throws {
        let model = ViewerModel()
        await model.load(url: try box(40, name: "limit-on-show.stl"))
        let shown = model.session.parts.map(\.info.name)
        #expect(shown.count == 1)

        let url = URL(fileURLWithPath: NSTemporaryDirectory() + "limit-fanout.3mf")
        try TestThreeMF.fanOut(depth: 8, fan: 10).write(to: url)       // 10^8 triangles, counted not built
        let (load, prompt) = try await loadUntilAsked(model, url)
        #expect(prompt.kind == .triangles && prompt.estimate == 1e8 && prompt.limit == 20_000_000)
        #expect(prompt.message(in: try lang("en")) == "This file expands to 100,000,000 triangles, which may need about 28 GB of memory.")
        let german = prompt.message(in: try lang("de"))
        #expect(german.contains("100.000.000") && german.contains("28 GB"))
        model.answerLimitPrompt(false)
        await load.value
        #expect(model.session.parts.map(\.info.name) == shown && model.status == nil)
    }

    @Test("the question is asked again for each file, and Cancel skips only that file")
    func severalFiles() async throws {
        let model = ViewerModel()
        let plates = try #require(model.features[PlatesFeature.self])
        let a = try box(3000, name: "limit-several-a.stl"), b = try box(2500, name: "limit-several-b.stl"), c = try box(30, name: "limit-several-c.stl")
        plates.loadFiles([a, b, c])

        #expect(await eventually { model.limitPrompt != nil })
        #expect(model.limitPrompt?.name == "limit-several-a.stl" && model.limitPrompt?.estimate == 3000)
        model.answerLimitPrompt(false)                                  // skips a
        #expect(await eventually { model.limitPrompt?.name == "limit-several-b.stl" })
        model.answerLimitPrompt(true)                                   // loads b
        // b, then c (no question: it is small), one to a plate; a was skipped
        #expect(await eventually { plates.board.plates.count == 2 && model.hasPart && !model.isLoading })
        #expect(model.limitPrompt == nil)
        #expect(plates.board.tabName(0).hasPrefix("limit-several-b") && plates.board.tabName(1).hasPrefix("limit-several-c"))
    }

    @Test("Add Model to Plate asks too")
    func addModel() async throws {
        let model = ViewerModel()
        await model.load(url: try box(40, name: "limit-add-base.stl"))
        let plates = try #require(model.features[PlatesFeature.self])
        plates.addModels([try box(3000, name: "limit-add-big.stl")])
        #expect(await eventually { model.limitPrompt?.name == "limit-add-big.stl" })
        model.answerLimitPrompt(false)
        #expect(await eventually { !model.isLoading })
        #expect(plates.board.modelCount == 1 && !model.addingModel)
    }

    // MARK: the session note

    @Test("a 3MF whose session is nested too deep loads with defaults and says so")
    func unreadableSession() async throws {
        let model = ViewerModel()
        let deep = "{\"v\":1,\"x\":" + String(repeating: "[", count: 200) + String(repeating: "]", count: 200) + "}"
        let url = URL(fileURLWithPath: NSTemporaryDirectory() + "limit-deep-session.3mf")
        try TestThreeMF.triangle(session: deep).write(to: url)
        await model.load(url: url)
        #expect(model.hasPart && model.limitPrompt == nil)
        #expect(model.status == "The settings saved in this file couldn't be read; defaults are used.")
    }

    @Test("a 3MF with a readable session, or none, says nothing")
    func readableSession() async throws {
        for (i, session) in [nil, "{\"v\":1}"].enumerated() {
            let model = ViewerModel()
            let url = URL(fileURLWithPath: NSTemporaryDirectory() + "limit-ok-session-\(i).3mf")
            try TestThreeMF.triangle(session: session).write(to: url)
            await model.load(url: url)
            #expect(model.hasPart && model.status == nil, "session \(session ?? "none")")
        }
    }
}
