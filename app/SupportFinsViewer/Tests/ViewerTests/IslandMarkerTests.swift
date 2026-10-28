import Metal
import Testing
import simd
@testable import Viewer

/// Show midair starts (`web/ui/islands.js`): the cone's size and place, the level-edge line, and the pixels.
@MainActor
@Suite("Midair start cones")
struct IslandMarkerTests {
    @Test func theConeIsSixPercentOfTheLongestSideClampedToOnePointFiveToSix() {
        #expect(IslandMarkers.unit(extent: 20) == 1.5)       // 0.03 * 20 = 0.6, held up to 1.5
        #expect(abs(IslandMarkers.unit(extent: 100) - 3) < 1e-5)
        #expect(IslandMarkers.unit(extent: 1000) == 6)
    }

    @Test func theTipIsOnTheStartAndTheBaseIsOneUnitBelow() {
        let m = IslandMarker(tip: SIMD3(4, -2, 30), points: [SIMD3(4, -2, 30)])
        let v = IslandMarkers.triangles([m], extent: 100)    // s = 3, radius 1.35
        #expect(v.count == 16 * 6 * 6)
        let pts = stride(from: 0, to: v.count, by: 6).map { SIMD3(v[$0], v[$0 + 1], v[$0 + 2]) }
        #expect(pts.map(\.z).max() == 30 && abs(pts.map(\.z).min()! - 27) < 1e-5)
        let wide = pts.map { hypot($0.x - 4, $0.y + 2) }.max()!
        #expect(abs(wide - 1.35) < 1e-4)
    }

    @Test func aLevelEdgeDrawsALineBetweenItsFarthestPointsOnlyWhenLongerThanTheCone() {
        let edge = IslandMarker(tip: SIMD3(0, 0, 5), points: [SIMD3(-10, 0, 5), SIMD3(0, 1, 5), SIMD3(10, 0, 5)])
        let line = IslandMarkers.lines([edge], extent: 100)
        #expect(line == [-10, 0, 5, 10, 0, 5] || line == [10, 0, 5, -10, 0, 5])
        let dot = IslandMarker(tip: SIMD3(0, 0, 5), points: [SIMD3(0, 0, 5), SIMD3(1, 0, 5)])
        #expect(IslandMarkers.lines([dot], extent: 100).isEmpty)
    }

    @Test func theConesDrawInRedAndAreGoneWhenCleared() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.camera = OrbitCamera(target: SIMD3(0, 0, 10), distance: 120, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        func red() -> Int {
            let px = ViewerRenderer.pixels(of: r.renderOffscreen(width: 256, height: 256))
            return stride(from: 0, to: px.count, by: 4).filter { px[$0 + 2] > 200 && px[$0 + 1] < 130 && px[$0] < 130 }.count
        }
        #expect(red() == 0)
        r.setIslandMarkers([IslandMarker(tip: SIMD3(0, 0, 10), points: [SIMD3(0, 0, 10)])], extent: 100)
        #expect(r.hasIslandMarkers)
        #expect(red() > 20, "the cone's pixels")
        r.setIslandMarkers([], extent: 100)
        #expect(!r.hasIslandMarkers && red() == 0)
    }
}
