import Foundation
import Testing
import simd
@testable import Viewer

/// The camera views, the glide, the view cube and the Mouse preset against `web/ui/viewpresets.js`,
/// `camviews.js`, `viewcube.js` and `navpresets.js`. The reference numbers were produced by running the web
/// module under node (checkout pinned at 299ef49).
@Suite("Camera views and view cube")
struct ViewsTests {
    func near(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ tol: Float = 1e-3) -> Bool { simd_length(a - b) <= tol }

    static let cells: [(String, Int, Int, [Int])] = [
        ("front", -1, -1, [-1, -1, 1]),
        ("front", 0, -1, [0, -1, 1]),
        ("front", 1, -1, [1, -1, 1]),
        ("front", -1, 0, [-1, -1, 0]),
        ("front", 0, 0, [0, -1, 0]),
        ("front", 1, 0, [1, -1, 0]),
        ("front", -1, 1, [-1, -1, -1]),
        ("front", 0, 1, [0, -1, -1]),
        ("front", 1, 1, [1, -1, -1]),
        ("back", -1, -1, [1, 1, 1]),
        ("back", 0, -1, [0, 1, 1]),
        ("back", 1, -1, [-1, 1, 1]),
        ("back", -1, 0, [1, 1, 0]),
        ("back", 0, 0, [0, 1, 0]),
        ("back", 1, 0, [-1, 1, 0]),
        ("back", -1, 1, [1, 1, -1]),
        ("back", 0, 1, [0, 1, -1]),
        ("back", 1, 1, [-1, 1, -1]),
        ("left", -1, -1, [-1, 1, 1]),
        ("left", 0, -1, [-1, 0, 1]),
        ("left", 1, -1, [-1, -1, 1]),
        ("left", -1, 0, [-1, 1, 0]),
        ("left", 0, 0, [-1, 0, 0]),
        ("left", 1, 0, [-1, -1, 0]),
        ("left", -1, 1, [-1, 1, -1]),
        ("left", 0, 1, [-1, 0, -1]),
        ("left", 1, 1, [-1, -1, -1]),
        ("right", -1, -1, [1, -1, 1]),
        ("right", 0, -1, [1, 0, 1]),
        ("right", 1, -1, [1, 1, 1]),
        ("right", -1, 0, [1, -1, 0]),
        ("right", 0, 0, [1, 0, 0]),
        ("right", 1, 0, [1, 1, 0]),
        ("right", -1, 1, [1, -1, -1]),
        ("right", 0, 1, [1, 0, -1]),
        ("right", 1, 1, [1, 1, -1]),
        ("top", -1, -1, [-1, 1, 1]),
        ("top", 0, -1, [0, 1, 1]),
        ("top", 1, -1, [1, 1, 1]),
        ("top", -1, 0, [-1, 0, 1]),
        ("top", 0, 0, [0, 0, 1]),
        ("top", 1, 0, [1, 0, 1]),
        ("top", -1, 1, [-1, -1, 1]),
        ("top", 0, 1, [0, -1, 1]),
        ("top", 1, 1, [1, -1, 1]),
        ("bottom", -1, -1, [-1, -1, -1]),
        ("bottom", 0, -1, [0, -1, -1]),
        ("bottom", 1, -1, [1, -1, -1]),
        ("bottom", -1, 0, [-1, 0, -1]),
        ("bottom", 0, 0, [0, 0, -1]),
        ("bottom", 1, 0, [1, 0, -1]),
        ("bottom", -1, 1, [-1, 1, -1]),
        ("bottom", 0, 1, [0, 1, -1]),
        ("bottom", 1, 1, [1, 1, -1]),
    ]
    static let lerp: [(String, Float, [Float])] = [
        ("back", 0, [120, -79.99999999999999, 60.00000000000001]),
        ("back", 0.1, [136.44975442139713, -53.257324871397735, 54.268977011574634]),
        ("back", 0.25, [149.251922039366, -7.214908400296551, 45.517127153746685]),
        ("back", 0.5, [135.05488589261793, 72.27918143417874, 30.592445599151137]),
        ("back", 0.8, [65.16397023501162, 141.4293914268553, 12.303829641207857]),
        ("back", 1, [9.564797266140736e-15, 156.20499351813308, 9.564797266140736e-15]),
        ("top", 0, [120, -79.99999999999999, 60.00000000000001]),
        ("top", 0.1, [105.351948849188, -86.29425878304001, 76.51318693378154]),
        ("top", 0.25, [81.08404598511876, -89.32153210961086, 99.23225981642715]),
        ("top", 0.5, [40.90487512429902, -76.43145830856984, 129.94238481685198]),
        ("top", 0.8, [7.114720410774879, -35.72904885222478, 151.89738582869632]),
        ("top", 1, [9.56479722511614e-19, -0.015620499284815056, 156.2049927371081]),
        ("bottom", 0, [120, -79.99999999999999, 60.00000000000001]),
        ("bottom", 0.1, [118.48625793750982, -97.05263088516948, 30.68865455234128]),
        ("bottom", 0.25, [104.49817736627753, -115.11435069151351, -15.126704598069185]),
        ("bottom", 0.5, [61.318434319286986, -114.5745425693113, -86.6759701858846]),
        ("bottom", 0.8, [11.685155868514531, -58.681083832856615, -144.29132868100484]),
        ("bottom", 1, [9.564797225127852e-19, -0.015620499284834185, -156.2049927371081]),
        ("home", 0, [120, -79.99999999999999, 60.00000000000001]),
        ("home", 0.1, [117.1314894300333, -82.77376227459537, 61.876639071710805]),
        ("home", 0.25, [112.67789311462636, -86.72518432693373, 64.6717465877349]),
        ("home", 0.5, [104.8905502382471, -92.73074182634718, 69.27468506643042]),
        ("home", 0.8, [95.03429419888514, -98.9359340216693, 74.70049454575016]),
        ("home", 1, [88.21430652798556, -102.44242048411226, 78.25462675869686]),
    ]
    static let ease: [(Float, Float)] = [
        (0, 0),
        (0.1, 0.02),
        (0.25, 0.125),
        (0.5, 0.5),
        (0.75, 0.875),
        (0.9, 0.98),
        (1, 1),
    ]
    static let dirPos: [[Float]] = [
        [1, 1.999000000005, 12.99999995],
        [0, -0.0004999999975, -4.999999975],
        [56.47342286643683, -65.5820394577976, 50.09739125248429],
    ]

    // MARK: presets and the glide

    @Test func presetDirectionsMatchTheWeb() {
        let want: [CameraView: [Float]] = [.front: [0, -1, 0], .back: [0, 1, 0], .left: [-1, 0, 0], .right: [1, 0, 0],
                                           .top: [0, -0.0001, 1], .bottom: [0, -0.0001, -1]]
        for v in CameraView.allCases { #expect(v.direction == SIMD3(want[v]!), "\(v)") }
        #expect(CameraView.allCases.map(\.label) == ["Front", "Back", "Left", "Right", "Top", "Bottom"])
        #expect(OrbitCamera.homeDirection == SIMD3(0.62, -0.72, 0.55))
    }

    @Test func dirPositionMatchesTheWeb() {
        let p = Self.dirPos
        #expect(near(OrbitCamera.position(alongFromTarget: [0, 0, 1], target: [1, 2, 3], distance: 10), SIMD3(p[0])))
        #expect(near(OrbitCamera.position(alongFromTarget: [0, 0, -1], target: .zero, distance: 5), SIMD3(p[1])))
        #expect(near(OrbitCamera.position(alongFromTarget: OrbitCamera.homeDirection, target: .zero, distance: 100), SIMD3(p[2])))
    }

    @Test func easeCurveMatchesTheWeb() {
        for (k, e) in Self.ease { #expect(abs(CameraGlide.ease(k) - e) < 1e-6, "k = \(k)") }
        #expect(CameraGlide.seconds == 0.45)
    }

    @Test func orbitLerpMatchesTheWeb() {
        let from = SIMD3<Float>(120, -80, 60)
        for (key, e, want) in Self.lerp {
            let dir: SIMD3<Float> = key == "home" ? OrbitCamera.homeDirection : CameraView(rawValue: key)!.direction
            let to = OrbitCamera.position(alongFromTarget: dir, target: .zero, distance: simd_length(from))
            let got = CameraGlide.orbitLerp(from, to, e)
            #expect(near(got, SIMD3(want), 2e-3), "\(key) at \(e): \(got) vs \(want)")
        }
    }

    @Test func aGlideTakesFourHundredFiftyMillisecondsAndKeepsTheDistance() {
        let from = SIMD3<Float>(120, -80, 60)
        let r = simd_length(from)
        #expect(near(CameraGlide.offset(from: from, toward: [0, 1, 0], after: 0), from))
        for t in [0.1, 0.225, 0.4] { #expect(abs(simd_length(CameraGlide.offset(from: from, toward: [0, 1, 0], after: t)) - r) < 1e-3) }
        let end = CameraGlide.offset(from: from, toward: [0, 1, 0], after: 0.45)
        #expect(near(simd_normalize(end), [0, 1, 0], 1e-4))
        #expect(near(CameraGlide.offset(from: from, toward: [0, 1, 0], after: 5), end))     // held at the end
    }

    // MARK: Home

    @Test func homeLooksFromTheHomeDirectionKeepingTargetAndZoom() {
        var c = OrbitCamera(target: [5, 6, 7], distance: 123, azimuth: 2, elevation: -0.3)
        c.setOffset(CameraGlide.offset(from: c.offset, toward: OrbitCamera.homeDirection, after: 0.45))
        #expect(near(c.direction, simd_normalize(OrbitCamera.homeDirection), 1e-4))
        #expect(abs(c.distance - 123) < 1e-2)
        #expect(c.target == [5, 6, 7])
    }

    @MainActor @Test func goHomeGlidesThereAndAViewButtonDoesToo() async throws {
        let c = try #require(ViewerController(), "no Metal device")
        c.show(GeneratedMesh.cube(edge: 40, device: c.renderer.device), name: "cube")
        c.renderer.camera.look(alongFromTarget: [-1, 1, 0.2])
        let d = c.renderer.camera.distance, target = c.renderer.camera.target
        c.goHome()
        try await Task.sleep(for: .milliseconds(900))
        #expect(near(c.renderer.camera.direction, simd_normalize(OrbitCamera.homeDirection), 1e-3))
        #expect(abs(c.renderer.camera.distance - d) < 1e-2 && c.renderer.camera.target == target)
        c.apply(.right)
        try await Task.sleep(for: .milliseconds(900))
        #expect(near(c.renderer.camera.direction, [1, 0, 0], 1e-3))
    }

    @MainActor @Test func aDragStopsAGlide() async throws {
        let c = try #require(ViewerController(), "no Metal device")
        c.show(GeneratedMesh.cube(edge: 40, device: c.renderer.device), name: "cube")
        c.apply(.back)
        try await Task.sleep(for: .milliseconds(100))
        c.orbit(dx: 0, dy: 0, viewHeight: 100)
        let az = c.renderer.camera.azimuth
        try await Task.sleep(for: .milliseconds(600))
        #expect(c.renderer.camera.azimuth == az)
    }

    // MARK: the cube

    @Test func cubeCellsMatchTheWebsDirections() {
        #expect(Self.cells.count == 54)
        for (face, col, row, want) in Self.cells {
            let got = ViewCube.cellDirection(ViewCube.Face(rawValue: face)!, col: col, row: row)
            #expect([got.x, got.y, got.z] == want, "\(face) \(col),\(row)")
        }
        #expect(Set(Self.cells.map { $0.3.map(String.init).joined(separator: ",") }).count == 26)
    }

    /// Every one of the 26 targets, hit at the centre of its cell on a cube turned to look from it, answers the
    /// web's view direction.
    @Test func everyTargetHitsItsDirection() {
        var seen = Set<[Int]>()
        for (face, col, row, want) in Self.cells {
            let f = ViewCube.Face(rawValue: face)!
            var cam = OrbitCamera()
            cam.look(alongFromTarget: SIMD3(Float(want[0]), Float(want[1]), Float(want[2])))
            let basis = ViewCube.Basis(camera: cam)
            let S = ViewCube.size, E = ViewCube.edge
            func off(_ i: Int) -> Float { Float(i) * (S / 2 - E / 2) }
            let p = f.n * (S / 2) + f.u * off(col) + f.v * off(row)
            let hit = ViewCube.hit(basis.project(p), basis)
            #expect(hit == ViewCube.Cell(face: f, col: col, row: row), "\(face) \(col),\(row)")
            if let d = hit?.direction { seen.insert([d.x, d.y, d.z]); #expect([d.x, d.y, d.z] == want) }
        }
        #expect(seen.count == 26)
    }

    @Test func aFacingCubeHitsOnlyWhatShowsAndMissesOutside() {
        var cam = OrbitCamera()
        cam.look(alongFromTarget: [0, -1, 0])          // Front, square on
        let basis = ViewCube.Basis(camera: cam)
        #expect(ViewCube.Face.allCases.filter { ViewCube.isVisible($0, basis) } == [.front])
        #expect(ViewCube.hit([0, 0], basis)?.direction == SIMD3(0, -1, 0))
        #expect(ViewCube.hit([31, 0], basis)?.direction == SIMD3(1, -1, 0))     // the right edge strip
        #expect(ViewCube.hit([-31, -31], basis)?.direction == SIMD3(-1, -1, 1)) // top left corner (Z up)
        #expect(ViewCube.hit([40, 0], basis) == nil)
        // the labels read upright: u is screen right, v is screen down
        let f = ViewCube.frame(.front, basis)
        #expect(f.u.x > 0.99 && abs(f.u.y) < 1e-3 && f.v.y > 0.99)
    }

    @Test func topKeepsTheBackOfThePlateAtTheTop() {
        var cam = OrbitCamera()
        cam.look(alongFromTarget: [0, -1e-4, 1])
        let basis = ViewCube.Basis(camera: cam)
        let f = ViewCube.frame(.top, basis)
        #expect(f.u.x > 0.99 && f.v.y > 0.99)         // x right, and the face's v (-Y) points down the screen
    }

    // MARK: the Mouse preset

    @Test func mousePresetsMatchTheWeb() {
        typealias B = NavPreset.Button
        func row(_ p: NavPreset) -> [NavAction] { [.left, .middle, .right].map { p.buttonAction($0) } }
        #expect(row(.default) == [.orbit, .dolly, .pan])
        #expect(row(.fusion) == [.none, .pan, .none])
        #expect(row(.onshape) == [.none, .pan, .orbit])
        #expect(row(.solidworks) == [.none, .orbit, .none])
        #expect(row(.blender) == [.none, .orbit, .none])
        #expect(NavPreset.allCases.map(\.label) == ["Default", "Fusion 360", "Onshape", "SolidWorks", "Blender"])
        #expect(NavPreset.fusion.hint == "Shift + middle-drag orbits, middle-drag pans, wheel zooms.")
    }

    @Test func optionDragOrbitsWhereTheLeftButtonIsLeftToPicking() {
        #expect(NavPreset.fusion.dragAction(.left, modifier: false, option: true) == .orbit)
        #expect(NavPreset.blender.dragAction(.left, modifier: true, option: true, shift: true) == .pan)
        #expect(NavPreset.onshape.dragAction(.left, modifier: false) == .none)
        #expect(NavPreset.default.dragAction(.left, modifier: false, option: true) == .orbit)
        #expect(NavPreset.fusion.dragAction(.middle, modifier: false, option: true) == .pan)      // a real button is not changed
    }

    @Test func modifierChordsSwapOrbitAndPan() {
        #expect(NavPreset.fusion.dragAction(.middle, modifier: true) == .orbit)       // Shift + middle orbits
        #expect(NavPreset.onshape.dragAction(.right, modifier: true) == .pan)         // Ctrl + right pans
        #expect(NavPreset.solidworks.dragAction(.middle, modifier: true) == .pan)
        #expect(NavPreset.blender.dragAction(.middle, modifier: true) == .pan)
        #expect(NavPreset.default.dragAction(.left, modifier: true) == .pan)          // Shift + left pans
        #expect(NavPreset.default.dragAction(.middle, modifier: true) == .dolly)
        #expect(NavPreset.fusion.dragAction(.left, modifier: true) == .none)
    }

    @Test func trackpadScrollFollowsThePreset() {
        #expect(NavPreset.default.scrollAction(modifier: false) == .orbit)
        #expect(NavPreset.default.scrollAction(modifier: true) == .pan)
        #expect(NavPreset.fusion.scrollAction(modifier: false) == .pan)
        #expect(NavPreset.fusion.scrollAction(modifier: true) == .orbit)
        #expect(NavPreset.blender.scrollAction(modifier: false) == .orbit)
        #expect(NavPreset.blender.scrollAction(modifier: true) == .pan)
    }

    @Test func storedKeyFallsBackToDefault() {
        #expect(NavPreset.storageKey == "sf.navPreset")
        #expect(NavPreset(storedKey: "onshape") == .onshape)
        #expect(NavPreset(storedKey: "toString") == .default)
        #expect(NavPreset(storedKey: nil) == .default)
    }

    @MainActor @Test func aDollyDragZoomsLikeTheWheel() throws {
        let c = try #require(ViewerController(), "no Metal device")
        let d = c.renderer.camera.distance
        c.navigate(.dolly, dx: 0, dy: 10, viewHeight: 100)
        #expect(c.renderer.camera.distance > d)           // dragging down moves away
        c.navigate(.none, dx: 50, dy: 50, viewHeight: 100)
        c.navigate(.dolly, dx: 0, dy: -10, viewHeight: 100)
        #expect(abs(c.renderer.camera.distance - d) < 1e-2)
    }
}
