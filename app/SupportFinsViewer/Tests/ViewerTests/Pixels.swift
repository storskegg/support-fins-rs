/// How many pixels of a BGRA readback (`ViewerRenderer.pixels(of:)`) satisfy `test(r, g, b)`.
func count(_ px: [UInt8], _ test: (Int, Int, Int) -> Bool) -> Int {
    var n = 0
    for i in stride(from: 0, to: px.count, by: 4) where test(Int(px[i + 2]), Int(px[i + 1]), Int(px[i])) { n += 1 }
    return n
}
