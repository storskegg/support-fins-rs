import Foundation
import Testing
import Viewer
@testable import Support_Fins

/// `sf.volume` (`web/ui/volume.js:25-37`) and the custom fields' range.
struct VolumeStoreTests {
    private func store() -> VolumeStore {
        VolumeStore(defaults: UserDefaults(suiteName: "VolumeStoreTests-\(UUID().uuidString)")!)
    }

    @Test func defaultsWhenNothingIsStored() {
        #expect(store().load() == .standard)
    }

    @Test func roundTripsACustomVolume() {
        let s = store()
        let v = PrinterVolume(x: 410, y: 400, z: 450)
        s.save(v)
        #expect(s.load() == v)
        #expect(s.defaults.string(forKey: "sf.volume")?.contains("\"x\":410") == true)
    }

    @Test func ignoresAStoredVolumeWithAZeroSide() {
        let s = store()
        s.defaults.set(#"{"x":0,"y":220,"z":270}"#, forKey: "sf.volume")
        #expect(s.load() == .standard)
        s.defaults.set("garbage", forKey: "sf.volume")
        #expect(s.load() == .standard)
    }

    @Test func clampsASideToTheFieldRange() {
        #expect(VolumeStore.clamped(5) == 20)
        #expect(VolumeStore.clamped(5000) == 2000)
        #expect(VolumeStore.clamped(250.4) == 250)
    }
}
