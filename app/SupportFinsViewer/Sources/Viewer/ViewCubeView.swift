import SwiftUI
import simd

/// The view cube (`web/ui/viewcube.js`, `#viewcube`): a little cube that turns with the camera, with Home and
/// the axis marker beside it. Click a face, edge or corner to look from there; drag it to orbit, whatever the
/// Mouse preset; Home goes back to the starting angle. Every jump keeps the target and the zoom.
public struct ViewCubeView: View {
    let controller: ViewerController

    public init(controller: ViewerController) { self.controller = controller }

    /// Only the cube and the axis marker follow the camera; the Home button beside them does not, so it is not
    /// re-evaluated for every frame of an orbit (the whole view was, and its glass button with it).
    public var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ViewCubeStage(controller: controller)
            VStack(spacing: 4) {
                Button { controller.goHome() } label: {
                    Image(systemName: "house").imageScale(.small).padding(4)
                }
                .buttonStyle(.plain)
                .glassEffect(.regular, in: .rect(cornerRadius: 6))
                .help("Home view: front right, from above. Keeps the zoom.")
                .accessibilityLabel("Home view: front right, from above. Keeps the zoom.")
                ViewCubeAxes(controller: controller)
            }
        }
    }
}

/// The cube itself: redraws with the camera, takes the clicks and drags.
struct ViewCubeStage: View {
    let controller: ViewerController
    @State private var hot: SIMD3<Int>?
    @State private var press: Press?

    private struct Press { var last: CGPoint; var start: CGPoint; var dragged = false }

    private static let accent = Color(red: 0.302, green: 0.639, blue: 1)    // #4da3ff

    var body: some View {
        let _ = controller.cameraRevision          // redraw with the camera
        let basis = ViewCube.Basis(camera: controller.renderer.camera)
        Canvas(rendersAsynchronously: true) { ctx, size in draw(&ctx, size, basis) } symbols: {
            // the labels are resolved once as symbols; resolving the text again for every camera change
            // was the largest cost of an orbit on the main thread
            ForEach(ViewCube.Face.allCases, id: \.self) { face in
                Text(face.label).font(.system(size: 11, weight: .semibold)).foregroundStyle(Color(white: 0.92)).tag(face)
            }
        }
        .frame(width: CGFloat(ViewCube.stage), height: CGFloat(ViewCube.stage))
        .contentShape(.rect)
        .gesture(drag(basis))
        // VoiceOver: one element, its faces as actions (a click on a face looks from there)
        .accessibilityElement()
        .accessibilityLabel("View cube")
        .accessibilityHint("Choose a face to look at the part from there")
        .accessibilityActions {
            ForEach(ViewCube.Face.allCases, id: \.self) { face in
                Button(face.label) { controller.glide(toward: face.n) }
            }
        }
        .onContinuousHover { phase in
            guard press == nil else { return }
            switch phase {
            case .active(let p): hot = ViewCube.hit(Self.local(p), basis)?.direction
            case .ended: hot = nil
            }
        }
    }

    /// A point of the stage in points from the cube's centre.
    private static func local(_ p: CGPoint) -> SIMD2<Float> {
        SIMD2(Float(p.x) - ViewCube.stage / 2, Float(p.y) - ViewCube.stage / 2)
    }

    // MARK: click and drag (`pointerdown` / `pointermove` / `pointerup`)

    private func drag(_ basis: ViewCube.Basis) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { g in
                var p = press ?? Press(last: g.startLocation, start: g.startLocation)
                defer { press = p }
                // a press moves the cube only after 4 px
                if !p.dragged {
                    guard hypot(g.location.x - p.start.x, g.location.y - p.start.y) >= 4 else { return }
                    p.dragged = true
                    hot = nil
                    controller.stopGlide()
                }
                // a tool holding the orbit (a ring drag) holds the cube's too
                guard controller.ringDragging == nil, controller.viewHeight > 0 else { return }
                controller.orbit(dx: g.location.x - p.last.x, dy: g.location.y - p.last.y, viewHeight: controller.viewHeight)
                p.last = g.location
            }
            .onEnded { g in
                let dragged = press?.dragged ?? false
                press = nil
                guard !dragged, let cell = ViewCube.hit(Self.local(g.location), basis) else { return }
                controller.glide(toward: SIMD3<Float>(cell.direction))
                hot = nil        // the cube turns away under a still pointer
            }
    }

    // MARK: drawing

    private func draw(_ ctx: inout GraphicsContext, _ size: CGSize, _ basis: ViewCube.Basis) {
        let c = CGPoint(x: size.width / 2, y: size.height / 2)
        let S = CGFloat(ViewCube.size), E = CGFloat(ViewCube.edge)
        let line = Color.white.opacity(0.22)
        for face in ViewCube.Face.allCases where ViewCube.isVisible(face, basis) {
            let f = ViewCube.frame(face, basis)
            let t = CGAffineTransform(a: CGFloat(f.u.x), b: CGFloat(f.u.y), c: CGFloat(f.v.x), d: CGFloat(f.v.y),
                                      tx: c.x + CGFloat(f.origin.x), ty: c.y + CGFloat(f.origin.y))
            var g = ctx
            g.concatenate(t)           // face coordinates: x along u, y along v, origin at its centre, S wide
            g.fill(Path(CGRect(x: -S / 2, y: -S / 2, width: S, height: S)), with: .color(Color(red: 0.17, green: 0.19, blue: 0.23).opacity(0.88)))
            for row in -1...1 {
                for col in -1...1 where hot != nil && ViewCube.Cell(face: face, col: col, row: row).direction == hot {
                    g.fill(Path(Self.rect(col: col, row: row, S, E)), with: .color(Self.accent.opacity(0.55)))
                }
            }
            g.stroke(Path(CGRect(x: -S / 2, y: -S / 2, width: S, height: S)), with: .color(line), lineWidth: 1)
            if let label = ctx.resolveSymbol(id: face) { g.draw(label, at: .zero) }
        }
    }

    /// The cell's rectangle in face coordinates.
    private static func rect(col: Int, row: Int, _ S: CGFloat, _ E: CGFloat) -> CGRect {
        func span(_ i: Int) -> (CGFloat, CGFloat) { i < 0 ? (-S / 2, E) : i > 0 ? (S / 2 - E, E) : (-S / 2 + E, S - 2 * E) }
        let x = span(col), y = span(row)
        return CGRect(x: x.0, y: y.0, width: x.1, height: y.1)
    }
}

/// The X, Y and Z marker beside the cube.
struct ViewCubeAxes: View {
    let controller: ViewerController

    private static let axes: [(name: String, color: Color)] = [
        ("X", Color(red: 1, green: 0.353, blue: 0.353)),        // #ff5a5a, the rotate rings' colours
        ("Y", Color(red: 0.353, green: 0.839, blue: 0.353)),    // #5ad65a
        ("Z", Color(red: 0.353, green: 0.549, blue: 1)),        // #5a8cff
    ]

    var body: some View {
        let _ = controller.cameraRevision
        let basis = ViewCube.Basis(camera: controller.renderer.camera)
        Canvas(rendersAsynchronously: true) { ctx, _ in drawAxes(&ctx, basis) } symbols: {
            ForEach(Self.axes, id: \.name) { axis in
                Text(axis.name).font(.system(size: 7, weight: .bold)).foregroundStyle(axis.color).tag(axis.name)
            }
        }
        .frame(width: 32, height: 32)
        .accessibilityHidden(true)
    }

    /// The axis marker: X, Y and Z as the camera sees them, the ones pointing away dimmer (`syncViewCube`).
    private func drawAxes(_ ctx: inout GraphicsContext, _ basis: ViewCube.Basis) {
        for (j, axis) in Self.axes.enumerated() {
            var e = SIMD3<Float>(0, 0, 0)
            e[j] = 1
            let p = basis.project(e), back = simd_dot(e, basis.back)
            var g = ctx
            g.opacity = Double(0.45 + 0.55 * max(0, back * 0.5 + 0.5))
            var path = Path()
            path.move(to: CGPoint(x: 16, y: 16))
            path.addLine(to: CGPoint(x: 16 + CGFloat(p.x) * 10, y: 16 + CGFloat(p.y) * 10))
            g.stroke(path, with: .color(axis.color), style: StrokeStyle(lineWidth: 1.6, lineCap: .round))
            if let label = ctx.resolveSymbol(id: axis.name) {
                g.draw(label, at: CGPoint(x: 16 + CGFloat(p.x) * 14.5, y: 16 + CGFloat(p.y) * 14.5))
            }
        }
    }
}
