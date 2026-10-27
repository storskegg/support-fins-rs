#if DEBUG
import SwiftUI
import Viewer

/// The debug-only frame-time overlay (the performance check of #89).
struct FrameOverlay: View {
    let controller: ViewerController

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.25)) { _ in
            let s = controller.renderer.stats.summary()
            VStack(alignment: .leading, spacing: 2) {
                Text(String(format: "%.0f fps", s.fps))
                Text(String(format: "GPU %.2f ms (p99 %.2f)", s.gpuMeanMs, s.gpuP99Ms))
                Text(String(format: "frame p99 %.2f ms, worst %.2f", s.intervalP99Ms, s.worstIntervalMs))
            }
            .font(.system(.caption, design: .monospaced))
            .padding(6)
            .background(.black.opacity(0.55), in: .rect(cornerRadius: 6))
            .foregroundStyle(.white)
            .padding(8)
        }
        .allowsHitTesting(false)
    }
}
#endif
