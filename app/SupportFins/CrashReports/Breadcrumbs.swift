import Foundation
import Synchronization

/// The last `capacity` user actions, oldest first, each stamped with its time. Thread-safe; never holds paths.
final class Breadcrumbs: Sendable {
    static let capacity = 50

    private let items = Mutex<[String]>([])

    var lines: [String] { items.withLock { $0 } }

    /// `text` as a line `<ISO time>  text`; the oldest fall off past `capacity`.
    func add(_ text: String, now: Date = Date()) {
        let line = "\(Self.stamp(now))  \(text)"
        items.withLock { list in
            list.append(line)
            if list.count > Self.capacity { list.removeFirst(list.count - Self.capacity) }
        }
    }

    func replace(with lines: [String]) {
        items.withLock { $0 = Array(lines.suffix(Self.capacity)) }
    }

    private static func stamp(_ date: Date) -> String {
        date.formatted(Date.ISO8601FormatStyle(includingFractionalSeconds: true))
    }
}
