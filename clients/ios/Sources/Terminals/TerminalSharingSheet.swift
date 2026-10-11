import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalSharingSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    @State private var saving: String?
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Group {
                if let session = model.terminals.session(sessionId) {
                    content(session)
                } else {
                    ContentUnavailableView("This terminal has ended.", systemImage: "moon.zzz")
                }
            }
            .navigationTitle("Sharing")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .accessibilityIdentifier("terminal.sharing.done")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
    }

    private func content(_ session: RuntimeSession) -> some View {
        let here = model.viewers(of: session)
        return List {
            Section {
                HStack(spacing: 12) {
                    AgentMark(kind: session.agent, size: 36, activity: session.mark(rested: .awake))
                        .padding(3)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(session.name)
                            .appTextStyle(.headline)
                            .foregroundStyle(theme.colors.ink)
                        Label("Encrypted end to end", systemImage: "lock.fill")
                            .appTextStyle(.caption)
                            .foregroundStyle(theme.colors.inkMuted)
                    }
                }
            }
            if !here.isEmpty {
                Section("Here now") {
                    HStack(spacing: 10) {
                        AvatarStack(people: here, size: 28, limit: 5, ring: theme.colors.raised)
                        Text(here.map(\.name).joined(separator: ", "))
                            .appTextStyle(.footnote)
                            .foregroundStyle(theme.colors.inkMuted)
                            .lineLimit(2)
                    }
                }
            }
            Section {
                Label(access(session), systemImage: "person.2.fill")
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .accessibilityIdentifier("terminal.sharing.access")
            } footer: {
                Text("Everyone you share with can type, resize and close this terminal.")
            }
            if session.isOwner {
                rooms(session)
            }
            Section {
                Text("To share it with people, open Kodosi on \(session.hostLabel).")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
            }
            if let failure {
                Section {
                    ErrorNote(message: failure)
                }
            }
        }
        .scrollContentBackground(.hidden)
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

    private func rooms(_ session: RuntimeSession) -> some View {
        Section {
            if model.rooms.missions.isEmpty {
                Text("You have no rooms yet.")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
            }
            ForEach(model.rooms.missions) { room in
                Button {
                    attach(session, to: room.id)
                } label: {
                    HStack(spacing: 10) {
                        RoomSigil(name: room.name, key: room.id, size: 26)
                        Text(room.name)
                            .appTextStyle(.body)
                            .foregroundStyle(theme.colors.ink)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        if saving == room.id {
                            ProgressView()
                        } else if session.missionId == room.id {
                            Image(systemName: "checkmark")
                                .font(.system(size: 14, weight: .bold))
                                .foregroundStyle(theme.colors.accent)
                                .transition(AnyTransition.pop)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .disabled(saving != nil || session.missionId == room.id)
                .accessibilityAddTraits(session.missionId == room.id ? .isSelected : [])
                .accessibilityIdentifier("terminal.sharing.room.\(AccessibilityIdentifier.token(room.id))")
            }
            if let room = session.missionName {
                Button("Stop sharing with \(room)", role: .destructive) {
                    attach(session, to: nil)
                }
                .disabled(saving != nil)
                .accessibilityIdentifier("terminal.sharing.stop")
            }
        } header: {
            Text("Share with a room")
        } footer: {
            Text("Everyone in the room gets full control, including people who join later.")
        }
        .animation(theme.motion.snappy, value: session.missionId)
    }

    private func attach(_ session: RuntimeSession, to roomId: String?) {
        guard session.missionId != roomId else { return }
        saving = roomId ?? ""
        failure = nil
        Task {
            do {
                try await model.terminals.share(session, inRoom: roomId)
            } catch {
                failure = error.localizedDescription
            }
            saving = nil
        }
    }
}
