import Foundation
import Synchronization

/// Frame timing for the debug overlay and the performance report: the interval between presented frames and
/// the GPU time of each, kept for the last few seconds.
public final class FrameStats: Sendable {
    public struct Summary: Sendable {
        public var frames = 0
        public var fps = 0.0
        public var gpuMeanMs = 0.0
        public var gpuP99Ms = 0.0
        public var intervalP99Ms = 0.0
        public var worstIntervalMs = 0.0
    }

    private struct State {
        var intervals: [Double] = []
        var gpu: [Double] = []
        var last: Double?
    }
    private let state = Mutex(State())
    private let keep = 4000

    public init() {}

    /// A frame was presented at `time` (seconds, any monotonic clock), the GPU took `gpuSeconds`.
    public func record(presentedAt time: Double, gpuSeconds: Double) {
        state.withLock { s in
            if let l = s.last, time > l { s.intervals.append(time - l) }
            s.last = time
            s.gpu.append(gpuSeconds)
            if s.intervals.count > keep { s.intervals.removeFirst(s.intervals.count - keep) }
            if s.gpu.count > keep { s.gpu.removeFirst(s.gpu.count - keep) }
        }
    }

    public func reset() { state.withLock { $0 = State() } }

    public func summary() -> Summary {
        state.withLock { s in
            var r = Summary()
            r.frames = s.intervals.count
            guard !s.intervals.isEmpty else { return r }
            let total = s.intervals.reduce(0, +)
            r.fps = Double(s.intervals.count) / total
            r.gpuMeanMs = s.gpu.reduce(0, +) / Double(s.gpu.count) * 1000
            r.gpuP99Ms = Self.percentile(s.gpu, 0.99) * 1000
            r.intervalP99Ms = Self.percentile(s.intervals, 0.99) * 1000
            r.worstIntervalMs = (s.intervals.max() ?? 0) * 1000
            return r
        }
    }

    static func percentile(_ v: [Double], _ p: Double) -> Double {
        let s = v.sorted()
        return s[min(s.count - 1, Int(Double(s.count) * p))]
    }
}
