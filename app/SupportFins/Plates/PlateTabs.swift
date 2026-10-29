import SwiftUI
import Viewer

/// The plate tabs (`#plates`, `ui/plates.js` `render`): one tab a plate with its name under "Plate N" (`name +n` for
/// several models), an × to delete it, + to add an empty plate, and See all to pull the view back to the whole row.
/// Under the toolbar, centred; held still while an export switches through the plates.
struct PlateTabs: View {
    let board: PlateBoard

    /// The strip as it is, or in a scroll view of the window's width when many plates make it wider than the window
    /// (the web's `#plates` scrolls sideways at `max-width`).
    var body: some View {
        ViewThatFits(in: .horizontal) {
            strip
            ScrollView(.horizontal, showsIndicators: false) { strip }.fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 12)
    }

    private var strip: some View {
        HStack(spacing: 4) {
            ForEach(board.plates.indices, id: \.self) { i in tab(i) }
            Button { board.addPlate() } label: { Text("+").font(.system(size: 16)).padding(.horizontal, 6) }
                .buttonStyle(.plain)
                .disabled(board.isEmpty)
                .help("Add a plate")
                .accessibilityLabel("Add a plate")
                .accessibilityIdentifier("plate-add")
            if board.plates.count > 1 {
                Divider().frame(height: 22)
                Button("See all") { board.frameAll() }
                    .buttonStyle(.plain)
                    .font(.system(size: 12))
                    .padding(.horizontal, 6)
                    .help("Pull the view back to show every plate")
                    .accessibilityIdentifier("plate-all")
            }
        }
        .padding(3)
        .background(.regularMaterial, in: .rect(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.separator))
        .padding(.top, 8)
        .disabled(board.busy)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("plates")
    }

    private func tab(_ i: Int) -> some View {
        let name = board.tabName(i), count = board.tabCount(i), current = i == board.active
        return HStack(spacing: 0) {
            Button { board.switchPlate(i) } label: {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Plate \(i + 1)").font(.system(size: 12))
                    if !name.isEmpty {
                        Text(count > 1 ? "\(PlateBoard.stem(name)) +\(count - 1)" : PlateBoard.stem(name))
                            .font(.system(size: 11)).foregroundStyle(.secondary)
                            .lineLimit(1).truncationMode(.tail).frame(maxWidth: 110, alignment: .leading)
                    }
                }
                .padding(.leading, 8).padding(.trailing, 4).padding(.vertical, 3)
                .foregroundStyle(current ? .primary : .secondary)
            }
            .buttonStyle(.plain)
            .help(name.isEmpty ? "Empty plate" : name)
            .accessibilityIdentifier("plate-tab-\(i + 1)")
            .accessibilityValue(current ? "selected" : "")
            // one empty plate has nothing to delete
            if board.plates.count > 1 || !name.isEmpty {
                Button { board.deletePlate(i) } label: { Text("×").font(.system(size: 14)).padding(.horizontal, 7).padding(.vertical, 2) }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .help("Delete plate \(i + 1)")
                    .accessibilityLabel("Delete plate \(i + 1)")
                    .accessibilityIdentifier("plate-x-\(i + 1)")
            }
        }
        .background(current ? AnyShapeStyle(Color.accentColor.opacity(0.2)) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 6))
    }
}

/// "Plate N" in front of each bed of the row (`label`, `ui/plateview.js`): the plate being edited in blue. A label
/// for one plate on its own would tell nothing apart, so there are none then.
struct PlateLabels: View {
    let board: PlateBoard

    var body: some View {
        if board.plates.count > 1, let c = board.host.controller {
            GeometryReader { geo in
                let _ = c.cameraRevision
                let v = c.volume, w = min(90, Double(v.x) * 0.45), live = board.plate.liveSlot.xy
                ForEach(board.plates.indices, id: \.self) { i in
                    let at = SIMD3<Float>(Float(Double(i - board.active) * board.pitch - live.x),
                                          Float(-live.y - Double(v.y) / 2 - w / 6), 0.05)
                    let n = c.renderer.camera.project(at)
                    Text("Plate \(i + 1)")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(i == board.active ? Color(red: 0.30, green: 0.64, blue: 1) : Color(red: 0.55, green: 0.58, blue: 0.65))
                        .position(x: CGFloat((n.x + 1) / 2) * geo.size.width, y: CGFloat((1 - n.y) / 2) * geo.size.height)
                }
            }
            .allowsHitTesting(false)
        }
    }
}
