import simd

/// The web app's colours (`web/ui/part.js:52-57`, `web/ui/scene.js:43,118-136`, `web/style.css`), as sRGB.
public enum Palette {
    static func hex(_ v: UInt32) -> SIMD3<Float> {
        SIMD3(Float((v >> 16) & 0xff), Float((v >> 8) & 0xff), Float(v & 0xff)) / 255
    }

    public static let background = hex(0x14161a)
    public static let plain = hex(0xb9c2d0)
    public static let overhang = hex(0xff5a4d)
    public static let small = hex(0xffb454)
    public static let bed = hex(0x3f7fd0)
    public static let hover = hex(0x4da3ff)
    /// The fins, the bed pad, the interface crest and the fin a removal click would drop (`web/ui/finbuild.js`
    /// finMaterial / padMaterial, `web/ui/remove.js` hoverFinMaterial).
    public static let fin = hex(0x59d98e)
    public static let pad = hex(0xe8b64c)
    public static let crest = hex(0x9b6bff)
    public static let removeHover = hex(0xff4d4d)
    /// The load arrow (`web/ui/strength.js` `0xffb454`) and the layer frames (`0x9ecbf5`).
    public static let loadArrow = hex(0xffb454)
    public static let layers = hex(0x9ecbf5)
    public static let gridMinor = hex(0x2b303a)
    public static let gridMajor = hex(0x3d4552)
    public static let plateEdge = hex(0x5b6472)
    public static let volumeBox = hex(0x39414f)
    /// Reverse side of a face (`web/ui/part.js`: `vec3(0.09, 0.10, 0.13)`).
    public static let backFace = SIMD3<Float>(0.09, 0.10, 0.13)
}
