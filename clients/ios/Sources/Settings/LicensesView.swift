import KodosiDesign
import SwiftUI

struct LicensesView: View {
    @Environment(\.theme) private var theme
    @State private var files: [LicenseFile]?

    var body: some View {
        List {
            if let files {
                if files.isEmpty {
                    EmptyState(title: "No notices in this copy", message: "This copy of Kodosi has no license files.") {
                        IconTile(symbol: "doc.text", tint: TileTint.graphite, size: 52)
                    } actions: {
                        EmptyView()
                    }
                    .bareRow()
                    .accessibilityIdentifier("settings.licenses.empty")
                } else {
                    Section {
                        ForEach(files) { file in
                            NavigationLink {
                                LicenseTextView(file: file)
                            } label: {
                                SettingRow(
                                    symbol: "doc.text",
                                    tint: TileTint.graphite,
                                    title: Text(file.title),
                                    subtitle: file.folder.map { Text($0) },
                                )
                            }
                            .raisedRows()
                            .accessibilityIdentifier("settings.licenses.\(file.url.lastPathComponent)")
                        }
                    } footer: {
                        Text("Kodosi uses these open-source projects.")
                            .appTextStyle(.footnote)
                            .foregroundStyle(theme.colors.inkFaint)
                    }
                }
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 40)
                    .bareRow()
            }
        }
        .groupedListChrome()
        .navigationTitle("Open-source licenses")
        .navigationBarTitleDisplayMode(.inline)
        .animation(theme.motion.soft, value: files)
        .task {
            files = await LicenseShelf.files(in: LicenseShelf.bundledFolder)
        }
    }
}

struct LicenseTextView: View {
    @Environment(\.theme) private var theme
    @State private var blocks: [LicenseBlock]?
    @State private var failed = false
    let file: LicenseFile

    var body: some View {
        ScrollView {
            Group {
                if let blocks {
                    LazyVStack(alignment: .leading, spacing: 14) {
                        ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                            switch block {
                            case let .heading(title):
                                Text(verbatim: title)
                                    .appTextStyle(.headline)
                                    .foregroundStyle(theme.colors.ink)
                                    .padding(.top, 6)
                            case let .text(text):
                                Text(text)
                                    .appTextStyle(.footnote)
                                    .foregroundStyle(theme.colors.ink)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                    }
                    .textSelection(.enabled)
                    .transition(.opacity)
                } else if failed {
                    ErrorNote(message: String(localized: "Kodosi cannot read this file."))
                } else {
                    ProgressView().frame(maxWidth: .infinity).padding(.top, 40)
                }
            }
            .frame(maxWidth: 680)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background { GroundBackdrop() }
        .navigationTitle(file.title)
        .navigationBarTitleDisplayMode(.inline)
        .animation(theme.motion.fade, value: blocks)
        .task {
            blocks = await LicenseShelf.blocks(of: file)
            failed = blocks == nil
        }
    }
}
