import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalSharingSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    @State private var room: String?
    @State private var saving = false
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Group {
                if let session = model.terminals.session(sessionId) {
                    content(session)
                } else {
                    TerminalStateCard(symbol: "moon.zzz", title: String(localized: "This terminal has ended."), message: nil, working: false) {
                        EmptyView()
                    }
                }
            }
            .background(theme.colors.surface)
            .navigationTitle("Sharing")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Done", symbol: "xmark", identifier: "terminal.sharing.done", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.medium, .large])
        .kodosiSheet()
        .onAppear { room = model.terminals.session(sessionId)?.missionId }
    }

    private func content(_ session: RuntimeSession) -> some View {
        let here = model.viewers(of: session)
        return ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(spacing: 11) {
                    AgentMark(session: session, size: 34, activity: session.mark(rested: .awake))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(session.isOwner ? String(localized: "Share \(session.name)") : session.name)
                            .appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                        Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                    }
                }
                if !here.isEmpty {
                    HStack(spacing: 8) {
                        AvatarStack(people: here, size: 22)
                        Text("Here now").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                }
                ListGroup {
                    ListRow(session.hostLabel, subtitle: hostState(session), symbol: DeviceGlyph.symbol(for: session.hostLabel),
                            tint: TileTint.graphite)
                    if let path = session.workingDir {
                        ListRow(HostPath.folderName(path), subtitle: path, monoSubtitle: true, symbol: "folder.fill", tint: TileTint.amber)
                    }
                    ListRow(access(session), symbol: "person.2.fill", tint: TileTint.blue)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("terminal.sharing.access")
                }
                if session.isOwner {
                    rooms(session)
                }
                VStack(alignment: .leading, spacing: 6) {
                    Text("Everyone you share with can type, resize and close this terminal.")
                    if session.isOwner {
                        Text("To share it with people, open Kodosi on \(session.hostLabel).")
                    }
                }
                .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(20)
            .frame(maxWidth: 640, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .animation(theme.motion.snappy, value: room)
        .animation(theme.motion.spring, value: failure)
    }

    @ViewBuilder
    private func rooms(_ session: RuntimeSession) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("A room").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            if model.rooms.missions.isEmpty {
                Text("You have no rooms yet.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
            FlowLayout(spacing: 6) {
                ForEach(model.rooms.missions) { mission in
                    let selected = room == mission.id
                    Button {
                        room = selected ? nil : mission.id
                    } label: {
                        RoomChoice(name: mission.name, key: mission.id, selected: selected)
                    }
                    .buttonStyle(PressScaleStyle())
                    .disabled(saving)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .accessibilityIdentifier("terminal.sharing.room.\(AccessibilityIdentifier.token(mission.id))")
                }
            }
        }
        if let failure {
            ErrorNote(message: failure).transition(AnyTransition.rise)
        }
        Button { save(session) } label: {
            Text(saving ? String(localized: "Sharing…") : actionTitle(session)).frame(maxWidth: .infinity).shimmer(saving)
        }
        .buttonStyle(.kodosi(.primary, size: .large))
        .disabled(saving || room == session.missionId)
        .accessibilityIdentifier("terminal.sharing.save")
    }

    private func actionTitle(_ session: RuntimeSession) -> String {
        guard room != session.missionId else {
            return session.missionId == nil ? String(localized: "Share") : String(localized: "Shared")
        }
        guard let room, let name = model.rooms.mission(room)?.name else { return String(localized: "Stop sharing") }
        return String(localized: "Share with \(name)")
    }

    private func hostState(_ session: RuntimeSession) -> String? {
        if session.status == .offline {
            return session.statusLabel
        }
        return session.isConnected ? nil : session.message ?? session.statusLabel
    }

    private func access(_ session: RuntimeSession) -> String {
        if !session.isOwner {
            let owner = session.ownerName ?? session.hostLabel
            return session.missionName.map { String(localized: "Shared by \(owner) in \($0)") } ?? String(localized: "Shared by \(owner)")
        }
        if let room = session.missionName {
            return String(localized: "Shared with \(room)")
        }
        let names = model.sharedNames(of: session)
        return names.isEmpty ? String(localized: "Only you and your devices") : names.joined(separator: ", ")
    }

    private func save(_ session: RuntimeSession) {
        guard room != session.missionId else { return }
        saving = true
        failure = nil
        Task {
            do {
                try await model.terminals.share(session, inRoom: room)
                dismiss()
            } catch {
                failure = error.localizedDescription
            }
            saving = false
        }
    }
}
