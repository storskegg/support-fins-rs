import CryptoKit
import Foundation
import Testing

@testable import Support_Fins

/// The Calibrate menu (`web/ui/calibrate.js`) in the app: its text against `parity/corpus/calibrate/cases.json`
/// (recorded by `parity/oracle/calibrate.js` from the page itself), and the bundled files against the checks of
/// `tests/calibrate.test.js`.
@MainActor
@Suite("Calibration")
struct CalibrationTests {
    private static func corpus() throws -> [String: Any] {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let data = try Data(contentsOf: root.appending(path: "parity/corpus/calibrate/cases.json"))
        return try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    @Test func theCouponsAreTheWebsWordForWord() throws {
        let c = try Self.corpus()
        let web = try #require(c["coupons"] as? [[String: Any]])
        #expect(web.count == CalibrationCatalog.coupons.count)
        for (w, ours) in zip(web, CalibrationCatalog.coupons) {
            #expect(w["name"] as? String == ours.name)
            #expect(w["file"] as? String == ours.file)
            #expect(w["sets"] as? String == ours.sets, "\(ours.name): sets")
            #expect(w["note"] as? String == ours.note, "\(ours.name): note")
            #expect(w["how"] as? String == ours.how, "\(ours.name): how")
        }
        #expect(c["intro"] as? String == CalibrationCatalog.intro)
        #expect(c["guide"] as? String == CalibrationCatalog.guide.absoluteString)
    }

    @Test func printTimesReadAsThePageWritesThem() throws {
        let pairs = try #require(try Self.corpus()["fmtTime"] as? [[Any]])
        #expect(pairs.count > 10)
        for p in pairs {
            let minutes = try #require(p[0] as? Int)
            #expect(CalibrationCatalog.fmtTime(minutes) == p[1] as? String, "\(minutes) min")
        }
    }

    @Test func eachRowCarriesItsTimeAndFilamentAndTheFooterNamesTheProfile() throws {
        let c = try Self.corpus()
        let estimates = try #require(CalibrationCatalog.loadEstimates())
        let rows = try #require(c["rows"] as? [[String: Any]])
        #expect(rows.count == CalibrationCatalog.coupons.count)
        for (row, coupon) in zip(rows, CalibrationCatalog.coupons) {
            let e = try #require(estimates.coupons[coupon.file])
            #expect(row["title"] as? String == coupon.name + CalibrationCatalog.detail(e), "\(coupon.name)")
            let download = try #require(row["download"] as? [String: Any])
            #expect(download["file"] as? String == coupon.file && download["title"] as? String == coupon.file)
        }
        #expect(c["profile"] as? String == CalibrationCatalog.profileNote(estimates))
        #expect(CalibrationCatalog.profileNote(nil) == "")
    }

    // MARK: the files (tests/calibrate.test.js)

    @Test func everyRowHasItsFileAndAnEstimateForThatExactFile() throws {
        let estimates = try #require(CalibrationCatalog.loadEstimates())
        #expect(estimates.profile.contains("mm layers"))
        for coupon in CalibrationCatalog.coupons {
            let e = try #require(estimates.coupons[coupon.file], "\(coupon.file): no estimate")
            #expect(e.minutes > 0 && e.grams > 0)
            let url = try #require(CalibrationCatalog.resourceURL(coupon.file), "\(coupon.file) is not in the app")
            let data = try Data(contentsOf: url)
            #expect(!data.isEmpty)
            let hash = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
            #expect(hash == e.sha256, "\(coupon.file): coupons.json's estimate is for another version")
        }
    }

    @Test func nothingIsBundledTheSheetDoesNotList() throws {
        let folder = try #require(CalibrationCatalog.resourceURL("coupons.json")).deletingLastPathComponent()
        let names = Set(try FileManager.default.contentsOfDirectory(atPath: folder.path))
        #expect(names == Set(CalibrationCatalog.coupons.map(\.file) + ["coupons.json"]))
        let estimates = try #require(CalibrationCatalog.loadEstimates())
        #expect(Set(estimates.coupons.keys) == Set(CalibrationCatalog.coupons.map(\.file)))
    }

    @Test func downloadWritesTheBundledBytes() throws {
        let model = CalibrationModel()
        let dir = try Scratch.directory()
        defer { Scratch.remove(dir) }
        for coupon in CalibrationCatalog.coupons {
            let dest = dir.appending(path: coupon.file)
            model.export(coupon, to: dest)
            #expect(model.error == nil, "\(model.error ?? "")")
            let source = try #require(CalibrationCatalog.resourceURL(coupon.file))
            #expect(try Data(contentsOf: dest) == Data(contentsOf: source), "\(coupon.file)")
        }
        // a file that is there is replaced (the Save panel asked)
        let dest = dir.appending(path: "angle-coupon.stl")
        model.export(CalibrationCatalog.coupons[0], to: dest)
        #expect(model.error == nil)
    }

    @Test func aFailedDownloadKeepsTheFileThatWasThere() throws {
        let model = CalibrationModel()
        let dir = try Scratch.directory()
        defer { Scratch.remove(dir) }
        let dest = dir.appending(path: "angle-coupon.stl")
        try Data("mine".utf8).write(to: dest)
        // a destination whose folder is gone cannot be written; nothing is created and the other file stays
        let gone = dir.appending(path: "no-folder").appending(path: "angle-coupon.stl")
        model.export(CalibrationCatalog.coupons[0], to: gone)
        #expect(model.error != nil)
        #expect(!FileManager.default.fileExists(atPath: gone.deletingLastPathComponent().path))
        #expect(try Data(contentsOf: dest) == Data("mine".utf8))
        let leftovers = try FileManager.default.contentsOfDirectory(atPath: dir.path).filter { $0.hasPrefix(".") }
        #expect(leftovers.isEmpty, "\(leftovers)")
    }
}
