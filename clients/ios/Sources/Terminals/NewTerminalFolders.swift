import KodosiBridge
import KodosiDesign
import SwiftUI

/// A page of the folders of one computer: its home folder with its recent folders, or `path`.
struct FolderRoute: Hashable {
    let computer: String
    let path: String?
}

/// The folders of a computer to start in: its recent folders over its home folder, or the folders
/// in one folder.
struct FolderPage: View {
    @Environment(\.theme) private var theme
    let computer: String
    let path: String?
    let current: String?
    let open: (String) -> Void
    let pick: (String, LauncherState) -> Void
    @State private var browser: FolderBrowser

    init(
        launcher: LauncherState, computer: String, path: String?, current: String?, open: @escaping (String) -> Void,
        pick: @escaping (String, LauncherState) -> Void,
    ) {
        self.computer = computer
        self.path = path
        self.current = current
        self.open = open
        self.pick = pick
        _browser = State(initialValue: FolderBrowser(launcher: launcher))
    }

    var body: some View {
        let listing = browser.listing
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if path == nil, let places = browser.launcher.places, !places.recent.isEmpty {
                    group(title: String(localized: "Recent folders")) {
                        ForEach(places.recent, id: \.self) { folder in
                            Button { pick(folder, browser.launcher) } label: {
                                SheetRow(title: places.name(of: folder), subtitle: places.shown(folder), monoSubtitle: true) {
                                    IconTile(symbol: "folder.fill", tint: TileTint.teal, size: 28)
                                } trailing: {
                                    if folder == current {
                                        Image(systemName: "checkmark").font(.system(size: 12, weight: .bold))
                                            .foregroundStyle(theme.colors.accentStrong)
                                    }
                                }
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .accessibilityAddTraits(folder == current ? .isSelected : [])
                            .accessibilityIdentifier("terminals.new.recent.\(AccessibilityIdentifier.token(folder))")
                        }
                    }
                }
                if let listing {
                    group(title: listing.shown(listing.path)) {
                        Button { pick(listing.path, browser.launcher) } label: {
                            SheetRow(title: String(localized: "Use \(listing.name(of: listing.path))"), titleColor: theme.colors.accentStrong) {
                                IconTile(symbol: "checkmark", tint: theme.colors.accent, size: 28)
                            }
                        }
                        .buttonStyle(ListRowButtonStyle())
                        .accessibilityIdentifier("terminals.new.folder.use")
                        if let parent = listing.parent {
                            Button { open(parent) } label: {
                                SheetRow(title: listing.name(of: parent), subtitle: String(localized: "Parent folder")) {
                                    IconTile(symbol: "arrow.up", tint: TileTint.graphite, size: 28)
                                }
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .accessibilityIdentifier("terminals.new.folder.up")
                        }
                        ForEach(listing.folders, id: \.self) { name in
                            Button { open(listing.child(name)) } label: {
                                SheetRow(title: name) {
                                    IconTile(symbol: "folder.fill", tint: TileTint.teal, size: 28)
                                } trailing: {
                                    Disclosure()
                                }
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .accessibilityIdentifier("terminals.new.folder.\(AccessibilityIdentifier.token(name))")
                        }
                    }
                } else if browser.failure == nil {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text("Asking \(computer) for its folders").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                    .padding(.leading, 4)
                }
                if let failure = browser.failure {
                    ErrorNote(message: failure)
                        .accessibilityIdentifier("terminals.new.folder.failure")
                }
            }
            .padding(20)
            .frame(maxWidth: 560, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(theme.colors.surface)
        .navigationTitle(listing.map { $0.name(of: $0.path) } ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .animation(theme.motion.soft, value: listing)
        .task { await browser.open(path) }
    }

    private func group(title: String, @ViewBuilder rows: @escaping () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(verbatim: title).appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
                .lineLimit(2).truncationMode(.middle)
                .padding(.leading, 4)
                .accessibilityAddTraits(.isHeader)
            ListGroup(content: rows)
        }
    }
}
