import Darwin

/// The process's memory as the system counts it against the app (`phys_footprint`, what Activity Monitor's Memory
/// column and the memory-pressure killer use), in bytes. Cheap: one `task_info` call.
enum MemoryFootprint {
    static func current() -> UInt64? {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
        }
        return kr == KERN_SUCCESS ? info.phys_footprint : nil
    }

    /// Megabytes, rounded.
    static func megabytes(_ bytes: UInt64) -> Int { Int((Double(bytes) / 1_048_576).rounded()) }
}
