import Foundation

/// A build volume in millimetres (`web/ui/volume.js`): listed by dimension, never by printer name.
public struct PrinterVolume: Hashable, Sendable, Codable {
    public var x: Float
    public var y: Float
    public var z: Float

    public init(x: Float, y: Float, z: Float) {
        self.x = x
        self.y = y
        self.z = z
    }

    /// The web app's presets, in its order (`VOLUMES`, `web/ui/volume.js:17-24`).
    public static let presets: [PrinterVolume] = [
        .init(x: 180, y: 180, z: 180),
        .init(x: 220, y: 220, z: 250),
        .init(x: 250, y: 220, z: 270),
        .init(x: 256, y: 256, z: 256),
        .init(x: 300, y: 300, z: 300),
        .init(x: 350, y: 350, z: 350),
    ]

    /// `DEFAULT_VOLUME`.
    public static let standard = PrinterVolume(x: 250, y: 220, z: 270)

    /// `volLabel`: `250 × 220 × 270 mm`.
    public var label: String { "\(Int(x)) × \(Int(y)) × \(Int(z)) mm" }

    public var isPreset: Bool { Self.presets.contains(self) }

    /// A volume the plate can draw: all three above zero (the custom fields allow 20...2000).
    public var isValid: Bool { x > 0 && y > 0 && z > 0 }
}
