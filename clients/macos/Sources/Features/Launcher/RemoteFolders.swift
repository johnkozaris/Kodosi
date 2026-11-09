import KodosiBridge
import KodosiDesign
import SwiftUI

struct RemoteFolders: View {
    let computer: String
    let current: String?
    let pick: (String) -> Void
    let close: () -> Void
    @State private var browser: FolderBrowser

    private static let up = "menu.up"
    private static let use = "menu.use"
    private static let folder = "folder:"

    init(launcher: LauncherState, computer: String, current: String?, pick: @escaping (String) -> Void, close: @escaping () -> Void) {
        self.computer = computer
        self.current = current
        self.pick = pick
        self.close = close
        _browser = State(initialValue: FolderBrowser(launcher: launcher))
    }

    var body: some View {
        Group {
            if let listing = browser.listing {
                PickerList(
                    rows: rows(in: listing), identifier: "launcher.list.browse", width: 320,
                    pick: { browse($0, in: listing) }, close: close,
                )
                .id(listing.path)
            } else {
                PickerList(rows: places, identifier: "launcher.list.folder", width: 320, pick: choose, close: close)
            }
        }
        .task { await browser.loadPlaces() }
    }

    private var places: [PickerRow] {
        let places = browser.launcher.places
        var rows = (places?.recent ?? []).map { folder in
            PickerRow(
                id: folder, title: name(folder, in: places), subtitle: places?.shown(folder), icon: .folder,
                current: folder == current,
            )
        }
        if places == nil {
            rows.append(PickerRow(
                id: "menu.asking", kind: .note, title: browser.failure ?? String(localized: "Asking \(computer) for its folders"),
            ))
        }
        rows.append(PickerRow(id: LauncherMenu.choose, kind: .action, title: String(localized: "Choose a folder…"), icon: .slot("folder")))
        return rows
    }

    private func rows(in listing: FolderList) -> [PickerRow] {
        var rows = [PickerRow(id: Self.use, kind: .action, title: String(localized: "Use \(name(listing.path, in: listing))"), icon: .slot("checkmark"))]
        if let failure = browser.failure {
            rows.append(PickerRow(id: "menu.failure", kind: .note, title: failure))
        }
        if let parent = listing.parent {
            rows.append(PickerRow(id: Self.up, title: name(parent, in: listing), icon: .slot("arrow.up")))
        }
        return rows + listing.folders.map { PickerRow(id: Self.folder + $0, title: $0, icon: .folder) }
    }

    private func name(_ folder: String, in places: FolderList?) -> String {
        places?.name(of: folder) ?? HostPath.folderName(folder)
    }

    private func choose(_ id: String) {
        if id == LauncherMenu.choose {
            open(nil)
        } else {
            pick(id)
        }
    }

    private func browse(_ id: String, in listing: FolderList) {
        switch id {
        case Self.use: pick(listing.path)
        case Self.up: open(listing.parent)
        default: open(listing.child(String(id.dropFirst(Self.folder.count))))
        }
    }

    private func open(_ path: String?) {
        Task { await browser.open(path) }
    }
}
