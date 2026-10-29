import Foundation

/// What a mouse button (or the trackpad's two-finger scroll) does to the camera.
public enum NavAction: Sendable, Equatable {
    case orbit, pan, dolly
    /// Left to the app: in the CAD presets a left drag no longer orbits, so left stays the button that picks.
    case none
}

/// Mouse navigation presets (`web/ui/navpresets.js`, #53): which button orbits, pans and zooms, matched to the
/// CAD packages people come from. The wheel zooms in every preset.
public enum NavPreset: String, CaseIterable, Sendable, Identifiable {
    case `default`, fusion, onshape, solidworks, blender

    public var id: String { rawValue }

    /// The web's storage key (`ui/navigation.js:10`); the value is the preset's key (`default`, `fusion`, ...).
    public static let storageKey = "sf.navPreset"

    /// The preset for a stored key; an unknown one falls back to Default (`navPreset`, navpresets.js:55).
    public init(storedKey: String?) { self = storedKey.flatMap(NavPreset.init(rawValue:)) ?? .default }

    public var label: String {
        switch self {
        case .default: "Default"
        case .fusion: "Fusion 360"
        case .onshape: "Onshape"
        case .solidworks: "SolidWorks"
        case .blender: "Blender"
        }
    }

    public var hint: String {
        switch self {
        case .default: "Left-drag orbits, right-drag pans, wheel zooms."
        case .fusion: "Shift + middle-drag orbits, middle-drag pans, wheel zooms."
        case .onshape: "Right-drag orbits, middle- or Ctrl + right-drag pans, wheel zooms."
        case .solidworks: "Middle-drag orbits, Ctrl + middle-drag pans, wheel zooms."
        case .blender: "Middle-drag orbits, Shift + middle-drag pans, wheel zooms."
        }
    }

    public enum Button: Sendable { case left, middle, right }

    /// The preset's `buttons` table (navpresets.js: `LEFT`, `MIDDLE`, `RIGHT`).
    public func buttonAction(_ button: Button) -> NavAction {
        switch (self, button) {
        case (.default, .left): .orbit
        case (.default, .middle): .dolly
        case (.default, .right): .pan
        case (.fusion, .middle), (.onshape, .middle): .pan
        case (.onshape, .right): .orbit
        case (.solidworks, .middle), (.blender, .middle): .orbit
        default: .none
        }
    }

    /// A drag with `button` down. OrbitControls swaps orbit and pan while Shift, Ctrl or Cmd is held
    /// (`onMouseDown`), which is how the chords in the hints come free.
    public func dragAction(_ button: Button, modifier: Bool, option: Bool = false, shift: Bool = false) -> NavAction {
        let a = swap(buttonAction(button), modifier)
        // The CAD presets leave the left button to picking and a trackpad has no middle one: Option-drag orbits there
        // and Option+Shift-drag pans (native; docs/UI-MAP.md section 6).
        if a == .none, button == .left, option { return shift ? .pan : .orbit }
        return a
    }

    /// A two-finger trackpad scroll, which has no button. Default keeps the Mac habit (scroll orbits, Shift or
    /// Option pans). The CAD presets play it as their middle button, so the scroll does what a middle drag does
    /// there (Fusion pans and Shift orbits, SolidWorks and Blender orbit and Shift pans); the wheel of a mouse
    /// and the pinch zoom in all of them. Native choice: the web has no trackpad scroll.
    public func scrollAction(modifier: Bool) -> NavAction {
        swap(self == .default ? .orbit : buttonAction(.middle), modifier)
    }

    private func swap(_ a: NavAction, _ modifier: Bool) -> NavAction {
        guard modifier else { return a }
        switch a {
        case .orbit: return .pan
        case .pan: return .orbit
        default: return a
        }
    }
}
