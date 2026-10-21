import KodosiBridge
import KodosiDesign
import SwiftUI

struct RemoteFolders: View {
    let launcher: LauncherState
    let computer: String
    let current: String?
    let pick: (String) -> Void
    let close: () -> Void
    @State private var listing: FolderList?
    @State private var failure: String?
    @State private var asking: UUID?

    private static let up = "menu.up"
    private static let use = "menu.use"
    private static let folder = "folder:"

    var body: some View {
        Group {
            if let listing {
                PickerList(
                    rows: rows(in: listing), identifier: "launcher.list.browse", width: 320,
                    pick: { browse($0, in: listing) }, close: close,
                )
                .id(listing.path)
            } else {
                PickerList(rows: places, identifier: "launcher.list.folder", width: 320, pick: choose, close: close)
            }
        }
        .task {
            guard launcher.places == nil else { return }
            do {
                _ = try await launcher.folders()
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    private var places: [PickerRow] {
        let places = launcher.places
        var rows = (places?.recent ?? []).map { folder in
            PickerRow(
                id: folder, title: name(folder, in: places), subtitle: places?.shown(folder), icon: .folder,
                current: folder == current,
            )
        }
        if places == nil {
            rows.append(PickerRow(id: "menu.asking", kind: .note, title: failure ?? String(localized: "Asking \(computer) for its folders")))
        }
        rows.append(PickerRow(id: LauncherMenu.choose, kind: .action, title: String(localized: "Choose a folder…"), icon: .slot("folder")))
        return rows
    }

    private func rows(in listing: FolderList) -> [PickerRow] {
        var rows = [PickerRow(id: Self.use, kind: .action, title: String(localized: "Use \(name(listing.path, in: listing))"), icon: .slot("checkmark"))]
        if let failure {
            rows.append(PickerRow(id: "menu.failure", kind: .note, title: failure))
        }
        if let parent = listing.parent {
            rows.append(PickerRow(id: Self.up, title: name(parent, in: listing), icon: .slot("arrow.up")))
        }
        return rows + listing.folders.map { PickerRow(id: Self.folder + $0, title: $0, icon: .folder) }
    }

    private func name(_ folder: String, in places: FolderList?) -> String {
        places?.shown(folder) == "~" ? "~" : HostPath.folderName(folder)
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
        let asked = UUID()
        asking = asked
        Task { @MainActor in
            do {
                let next = try await launcher.folders(at: path)
                guard asking == asked else { return }
                failure = nil
                listing = next
            } catch {
                guard asking == asked else { return }
                failure = error.localizedDescription
            }
        }
    }
}
