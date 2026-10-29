import SwiftUI

/// The Calibrate menu (`#calibrate-menu`, `web/ui/calibrate.js`) as a sheet: the intro line, one collapsible row per
/// coupon (its print time and filament where a plugin shows its version, and a Download button), and the profile
/// the numbers came from with the link to the full guide.
struct CalibrationSheet: View {
    let model: CalibrationModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var open: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Calibration prints").font(.title3.bold())
            Text(JS.t(CalibrationCatalog.intro)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("calibrate-intro")
            List {
                ForEach(CalibrationCatalog.coupons) { coupon in row(coupon) }
            }
            .frame(minHeight: 280)
            .accessibilityIdentifier("calibrate-list")
            if let error = model.error {
                Text(error).foregroundStyle(.red).font(.caption).accessibilityIdentifier("calibrate-error")
            }
            HStack {
                Text(CalibrationCatalog.profileNote(model.estimates)).font(.caption).foregroundStyle(.secondary)
                    .accessibilityIdentifier("calibrate-profile")
                Spacer()
                Button("Full guide") { openURL(CalibrationCatalog.guide) }
                    .buttonStyle(.link)
                    .accessibilityIdentifier("calibrate-guide")
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("calibrate-done")
            }
        }
        .padding(20)
        .frame(width: 560)
    }

    private func row(_ c: Coupon) -> some View {
        DisclosureGroup(isExpanded: Binding(get: { open.contains(c.name) }, set: { if $0 { open.insert(c.name) } else { open.remove(c.name) } })) {
            VStack(alignment: .leading, spacing: 6) {
                Text(JS.t(c.sets)).fontWeight(.medium)
                if let note = c.note { Text(JS.t(note)).foregroundStyle(.orange) }
                Text(JS.t(c.how)).foregroundStyle(.secondary)
            }
            .font(.callout)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.vertical, 4)
        } label: {
            // the name is the label alone: a Download button inside a disclosure label is folded into it by VoiceOver and
            // the accessibility tree, so the button sits beside the group instead (the overlay below)
            (Text(JS.t(c.name)).bold()
                + Text(model.estimates?.coupons[c.file].map(CalibrationCatalog.detail) ?? "").foregroundStyle(.secondary))
                .accessibilityIdentifier("calibrate-row-\(c.name.lowercased())")
        }
        .overlay(alignment: .topTrailing) {
            Button("Download") { model.download(c, from: NSApp.keyWindow) }
                .help(c.file)
                .accessibilityIdentifier("calibrate-download-\(c.name.lowercased())")
        }
    }
}
