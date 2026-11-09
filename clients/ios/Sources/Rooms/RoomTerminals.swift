import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomTerminals: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openRoute) private var openRoute
    let roomId: String
    @State private var moving: String?
    @State private var added = 0

    var body: some View {
        let sessions = model.terminals.sessions.filter { $0.missionId == roomId }
        let mine = model.terminals.sessions.filter { $0.isOwner && $0.missionId != roomId && $0.status != .closing }
        Group {
            if sessions.isEmpty {
                EmptyState(
                    title: "No terminals here yet",
                    message: "Share one of your terminals with this room. Everyone in the room gets full control.",
                ) {
                    TerminalGhost()
                } actions: {
                    if !mine.isEmpty {
                        addMenu(mine) {
                            Label("Add one of my terminals", systemImage: "plus")
                        }
                        .buttonStyle(.kodosi(.primary))
                    }
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(sessions) { session in
                            card(session)
                        }
                        if !mine.isEmpty {
                            addMenu(mine) {
                                DashedSlot(radius: Radius.lg) {
                                    HStack(spacing: 10) {
                                        Image(systemName: "plus").font(.system(size: 14, weight: .semibold))
                                        Text("Add one of my terminals").appTextStyle(.subhead)
                                    }
                                    .foregroundStyle(theme.colors.inkMuted)
                                    .multilineTextAlignment(.center)
                                    .padding(.vertical, 10)
                                    .frame(maxWidth: .infinity, minHeight: 56)
                                }
                            }
                            .buttonStyle(PressScaleStyle(scale: 0.98))
                        }
                        Text("Everyone in the room can type, resize and close these terminals.")
                            .appTextStyle(.caption).fontWeight(.regular)
                            .foregroundStyle(theme.colors.inkFaint)
                            .padding(.leading, 4)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .frame(maxWidth: 760)
                    .frame(maxWidth: .infinity)
                }
            }
        }
        .animation(theme.motion.spring, value: sessions.map(\.id))
        .sensoryFeedback(.success, trigger: added)
    }

    private func card(_ session: RuntimeSession) -> some View {
        TerminalListCard(session: session, sign: model.terminals.sign(of: session), viewers: model.viewers(of: session), badged: false) {
            openRoute(TerminalRoute(sessionId: session.id))
        }
        .disabled(!session.canOpen || moving == session.id)
        .contextMenu {
            if session.isOwner {
                Button(role: .destructive) { move(session, to: nil) } label: {
                    Label("Remove from room", systemImage: "minus.circle")
                }
            }
        }
        .accessibilityIdentifier("rooms.terminal.\(AccessibilityIdentifier.token(session.id))")
    }

    private func addMenu(_ mine: [RuntimeSession], @ViewBuilder label: () -> some View) -> some View {
        Menu {
            ForEach(mine) { session in
                Button { move(session, to: roomId) } label: {
                    Text(session.name)
                    Text(session.missionName.map { String(localized: "In \($0)") } ?? session.hostLabel)
                }
            }
        } label: {
            label()
        }
        .menuStyle(.button)
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
                rooms.report(error)
            }
        }
    }
}
