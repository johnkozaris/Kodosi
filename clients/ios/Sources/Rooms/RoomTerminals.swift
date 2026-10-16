import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomTerminals: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    @State private var moving: String?
    @State private var added = 0

    var body: some View {
        let sessions = model.terminals.sessions.filter { $0.missionId == roomId }
        let mine = model.terminals.sessions.filter { $0.isOwner && $0.missionId != roomId && $0.status != .closing }
        List {
            if sessions.isEmpty {
                EmptyState(
                    title: "No terminals here yet",
                    message: "Share one of your terminals with this room. Everyone in the room gets full control."
                ) {
                    TerminalGhost()
                } actions: {
                    if !mine.isEmpty {
                        addMenu(mine)
                            .menuStyle(.button)
                            .buttonStyle(.kodosi(.primary))
                    }
                }
                .frame(minHeight: 320)
                .listRowBackground(Color.clear)
            } else {
                Section {
                    ForEach(sessions) { session in
                        terminalLink(session)
                    }
                } footer: {
                    Text("Everyone in the room can type, resize and close these terminals.")
                }
                if !mine.isEmpty {
                    Section {
                        addMenu(mine)
                            .foregroundStyle(theme.colors.accentStrong)
                            .listRowBackground(theme.colors.raised)
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .animation(theme.motion.spring, value: sessions.map(\.id))
        .sensoryFeedback(.success, trigger: added)
    }

    private func terminalLink(_ session: RuntimeSession) -> some View {
        NavigationLink(value: TerminalRoute(sessionId: session.id)) {
            RoomTerminalRow(session: session, moving: moving == session.id)
        }
        .disabled(!session.canOpen)
        .listRowBackground(theme.colors.raised)
        .contextMenu {
            if session.isOwner {
                Button(role: .destructive) { move(session, to: nil) } label: {
                    Label("Remove from room", systemImage: "minus.circle")
                }
            }
        }
        .accessibilityIdentifier("rooms.terminal.\(AccessibilityIdentifier.token(session.id))")
    }

    private func addMenu(_ mine: [RuntimeSession]) -> some View {
        Menu {
            ForEach(mine) { session in
                Button { move(session, to: roomId) } label: {
                    Text(session.name)
                    Text(session.missionName.map { String(localized: "In \($0)") } ?? session.hostLabel)
                }
            }
        } label: {
            Label("Add one of my terminals", systemImage: "plus")
        }
        .disabled(moving != nil)
        .accessibilityIdentifier("rooms.terminals.add")
    }

    private func move(_ session: RuntimeSession, to room: String?) {
        guard moving == nil else { return }
        let terminals = model.terminals
        let rooms = model.rooms
        guard let current = terminals.session(session.id), current.incarnationId == session.incarnationId else {
            model.notices.show(String(localized: "This terminal changed. Try again."))
            return
        }
        moving = session.id
        Task {
            defer { moving = nil }
            do {
                try await terminals.mutate("session.attachMission", session: current, fields: ["missionId": room.map(JSONValue.string) ?? .null])
                if room != nil {
                    added += 1
                }
            } catch {
                rooms.report(error, announcedByRuntime: true)
            }
        }
    }
}

private struct RoomTerminalRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let moving: Bool

    var body: some View {
        HStack(spacing: 12) {
            AgentMark(kind: session.agent, size: 36, activity: session.mark(rested: session.canOpen ? .awake : .asleep))
            VStack(alignment: .leading, spacing: 3) {
                Text(session.name)
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                HStack(spacing: 5) {
                    DeviceGlyph(label: session.hostLabel).font(.system(size: 10, weight: .medium))
                    Text(session.activity ?? place)
                        .lineLimit(1)
                        .shimmer(session.isWorking)
                        .id(session.activity ?? place)
                        .transition(.blurReplace)
                    if let progress = session.progress {
                        Text(Double(progress) / 100, format: .percent)
                            .monospacedDigit()
                            .contentTransition(.numericText(value: Double(progress)))
                    }
                }
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkMuted)
            }
            Spacer(minLength: 8)
            if !session.isOwner, let owner = session.ownerName {
                PersonAvatar(name: owner, key: session.ownerUserId, size: 24)
            }
            if moving {
                ProgressView().controlSize(.small)
            } else if let sign = model.terminals.sign(of: session) {
                StatusSign(form: sign, size: 13).transition(AnyTransition.pop)
            } else if session.isTroubled {
                Circle().fill(theme.colors.caution).frame(width: 7, height: 7)
                    .accessibilityLabel(Text(session.statusLabel))
            }
        }
        .padding(.vertical, 4)
        .opacity(session.canOpen ? 1 : 0.6)
        .animation(theme.motion.soft, value: session.activity)
        .animation(theme.motion.snappy, value: model.terminals.sign(of: session))
        .accessibilityElement(children: .combine)
    }

    private var place: String {
        [session.hostLabel, session.folderName].compactMap(\.self).joined(separator: " · ")
    }
}
