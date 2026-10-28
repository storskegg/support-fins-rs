import Metal

/// What a segment of the support buffer is drawn as.
public enum SupportKind: Equatable, Sendable {
    /// A fin wall, wedge or sway brace (`Palette.fin`).
    case fin
    /// Interface-crest material (`Palette.crest`).
    case crest
}

/// A run of vertices of the fin buffer and how to draw it.
public struct SupportSegment: Equatable, Sendable {
    public var range: Range<Int>
    public var kind: SupportKind
    public init(range: Range<Int>, kind: SupportKind) { (self.range, self.kind) = (range, kind) }
}

/// The fins and the bed pad of a build, as the Rust allocations the engine returned (`BuildResult.fins()` /
/// `pad()` wrapped with no copy): interleaved position + normal, `f32`, three vertices a triangle, in print
/// space, so they are drawn with no model matrix. The part is what moves (it draws seated with the build's
/// offset); the supports are already where they stand.
@MainActor
public struct SupportLayer {
    public var fins: MTLBuffer?
    public var finVertices: Int
    public var pad: MTLBuffer?
    public var padVertices: Int
    /// Floats between two vertices (`GpuBuffer::stride / 4`): 6.
    public var stride: Int

    public init(fins: MTLBuffer?, finVertices: Int, pad: MTLBuffer?, padVertices: Int, stride: Int = 6) {
        (self.fins, self.finVertices, self.pad, self.padVertices, self.stride) = (fins, finVertices, pad, padVertices, stride)
    }
}

/// Which vertex ranges of the fin buffer are drawn, and as what.
public enum SupportSegments {
    /// The fin buffer minus the `hidden` ranges (removed fins), with the `crest` ranges drawn as the interface
    /// material. Ranges are `[from, to)` vertices; they may overlap each other and may stick out of
    /// `[0, total)`; the result is sorted, disjoint, and within it, in whole runs (a triangle is never split:
    /// the inputs are triangle-aligned by construction, `triRanges`).
    public static func fins(total: Int, hidden: [Range<Int>], crest: [Range<Int>]) -> [SupportSegment] {
        let hide = merged(hidden, within: total)
        let cr = merged(crest, within: total)
        var out: [SupportSegment] = []
        var at = 0
        func emit(_ from: Int, _ to: Int) {
            guard from < to else { return }
            // split [from, to) at the crest ranges
            var p = from
            for c in cr where c.upperBound > p && c.lowerBound < to {
                if c.lowerBound > p { out.append(SupportSegment(range: p..<c.lowerBound, kind: .fin)) }
                let lo = max(c.lowerBound, p), hi = min(c.upperBound, to)
                out.append(SupportSegment(range: lo..<hi, kind: .crest))
                p = hi
            }
            if p < to { out.append(SupportSegment(range: p..<to, kind: .fin)) }
        }
        for h in hide {
            emit(at, h.lowerBound)
            at = h.upperBound
        }
        emit(at, total)
        return out
    }

    /// Sorted, non-overlapping, clipped to `[0, total)`, empty ranges dropped.
    static func merged(_ ranges: [Range<Int>], within total: Int) -> [Range<Int>] {
        let clipped = ranges.compactMap { r -> Range<Int>? in
            let lo = max(r.lowerBound, 0), hi = min(r.upperBound, total)
            return lo < hi ? lo..<hi : nil
        }.sorted { $0.lowerBound < $1.lowerBound }
        var out: [Range<Int>] = []
        for r in clipped {
            if let last = out.last, r.lowerBound <= last.upperBound {
                out[out.count - 1] = last.lowerBound..<max(last.upperBound, r.upperBound)
            } else {
                out.append(r)
            }
        }
        return out
    }
}
