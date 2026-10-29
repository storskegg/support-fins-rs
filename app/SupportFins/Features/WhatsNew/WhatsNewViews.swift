import SwiftUI

/// The releases as the page's card lists them (`#whatsnew .list`): the newest release's lines, then each older release
/// under its "v1.0.0 2026-10-28" line. Scrolls past `maxHeight`, with the last line fading until the end is reached
/// (the page does it because macOS hides the scrollbar; the same here).
struct WhatsNewList: View {
    let changelog: Changelog
    var maxHeight: CGFloat?
    @State private var atEnd = true

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                if let newest = changelog.newest { lines(newest) }
                ForEach(changelog.older) { release in
                    (Text(release.tag) + Text(" ") + Text(release.date).fontWeight(.regular).foregroundStyle(.secondary))
                        .fontWeight(.semibold)
                        .padding(.top, 6)
                        .accessibilityIdentifier("whatsnew-release-\(release.tag)")
                    lines(release)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onScrollGeometryChange(for: Bool.self) { g in
            g.contentOffset.y + g.containerSize.height >= g.contentSize.height - 2
        } action: { _, end in atEnd = end }
        .frame(maxHeight: maxHeight)
        .fixedSize(horizontal: false, vertical: maxHeight != nil)
        .mask {
            if atEnd { Rectangle() } else {
                LinearGradient(stops: [.init(color: .black, location: 0.8), .init(color: .clear, location: 1)], startPoint: .top, endPoint: .bottom)
            }
        }
        .accessibilityIdentifier("whatsnew-list")
    }

    private func lines(_ release: Changelog.Release) -> some View {
        ForEach(Array(release.lines.enumerated()), id: \.offset) { _, line in
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text("•").foregroundStyle(.secondary)
                Text(line).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// "What's new v1.0.1" over the list, and the "See all releases" link: the page's `#whatsnew` section.
struct WhatsNewSection: View {
    let changelog: Changelog
    var listHeight: CGFloat?
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            (Text("What's new").fontWeight(.semibold) + Text(" ") + Text(changelog.newest?.tag ?? "").foregroundStyle(.secondary))
                .font(.headline)
                .help("Version")
                .accessibilityIdentifier("whatsnew-heading")
            WhatsNewList(changelog: changelog, maxHeight: listHeight)
            Button("See all releases") { openURL(Changelog.releasesURL) }
                .buttonStyle(.link)
                .accessibilityIdentifier("whatsnew-all")
        }
        .font(.callout)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("whatsnew")
    }
}

/// The card under the empty window's intro (`index.html` `#drop`): nothing while nothing is released.
struct WhatsNewCard: View {
    var changelog: Changelog = .bundled

    var body: some View {
        if !changelog.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Divider().padding(.bottom, 12)
                WhatsNewSection(changelog: changelog, listHeight: 112)
            }
            .frame(maxWidth: 460, alignment: .leading)
            .foregroundStyle(.secondary)
        }
    }
}

/// Help > What's new: the same section in a sheet, with the whole list showing.
struct WhatsNewSheet: View {
    let changelog: Changelog
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            WhatsNewSection(changelog: changelog, listHeight: 300)
            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("whatsnew-done")
            }
        }
        .padding(20)
        .frame(width: 480)
    }
}
