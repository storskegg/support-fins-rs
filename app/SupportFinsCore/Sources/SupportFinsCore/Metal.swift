import Foundation
import Metal

/// Metal requires a `bytesNoCopy` buffer's length to be a whole number of pages (16 KiB on Apple
/// silicon); `GpuBuffer` allocates exactly that many.
private let pageSize = 16384

extension GpuBuffer {
    /// The buffer's memory, shared with the GPU without a copy.
    ///
    /// The `MTLBuffer` points at the Rust allocation itself (`contents()` equals `address()`), and its
    /// deallocator keeps a strong reference to this `GpuBuffer` until Metal releases the buffer, so
    /// the memory cannot be freed while the GPU can still read it.
    ///
    /// This is the one place the app turns an integer address into a pointer.
    public func metalBuffer(device: MTLDevice) -> MTLBuffer? {
        guard let pointer = UnsafeMutableRawPointer(bitPattern: UInt(address())) else { return nil }
        let length = max(1, (Int(lenBytes()) + pageSize - 1) / pageSize) * pageSize
        let owner = self
        return device.makeBuffer(
            bytesNoCopy: pointer,
            length: length,
            options: .storageModeShared,
            deallocator: { _, _ in withExtendedLifetime(owner) {} }
        )
    }
}
