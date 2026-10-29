import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalTitle: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession

    var body: some View {
        HStack(spacing: 9) {
            AgentMark(session: session, size: 26, activity: session.mark(rested: session.isConnected ? .awake : .asleep))
                .padding(2)
            VStack(alignment: .leading, spacing: 0) {
                Text(session.name)
                    .appTextStyle(.subhead)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                    .contentTransition(.interpolate)
                caption
                    .appTextStyle(.caption2)
                    .lineLimit(1)
            }
        }
        .animation(theme.motion.soft, value: session.name)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("terminal.header")
    }

    @ViewBuilder
    private var caption: some View {
        if session.isConnected, session.status == .reconnecting {
            Text("Reconnecting")
                .foregroundStyle(theme.colors.caution)
                .shimmer()
        } else if session.isConnected, session.status == .offline {
            Text(session.statusLabel)
                .foregroundStyle(theme.colors.caution)
        } else if session.isConnected, let notice = session.message {
            Text(notice)
                .foregroundStyle(theme.colors.caution)
        } else if !session.isConnected {
            Text(session.statusLabel)
                .foregroundStyle(session.isTroubled ? theme.colors.caution : theme.colors.inkFaint)
        } else if session.activity != nil || session.progress != nil {
            TerminalCaption(session: session)
        } else if let runsIn = session.place {
            SessionPlace(place: runsIn)
                .foregroundStyle(theme.colors.inkMuted)
        } else {
            HStack(spacing: 4) {
                DeviceGlyph(label: session.hostLabel)
                Text(session.hostLabel)
            }
            .foregroundStyle(theme.colors.inkMuted)
        }
    }
}

struct TerminalMenu: View {
    let session: RuntimeSession
    @Binding var prompt: TerminalPrompt?
    @Binding var showsSharing: Bool
    let interrupt: () -> Void
    let openBrowser: () -> Void

    var body: some View {
        Menu {
            if session.isOwner {
                Button("Rename", systemImage: "pencil") { prompt = .rename(session) }
                    .accessibilityIdentifier("terminal.menu.rename")
                Button("Share with a room", systemImage: "person.2") { showsSharing = true }
                    .accessibilityIdentifier("terminal.menu.sharing")
            } else {
                Button("Sharing", systemImage: "person.2") { showsSharing = true }
                    .accessibilityIdentifier("terminal.menu.sharing")
            }
            if session.canControl {
                Button("Interrupt", systemImage: "stop.circle", action: interrupt)
                    .accessibilityIdentifier("terminal.menu.interrupt")
                Button("Open Browser", systemImage: "globe", action: openBrowser)
                    .accessibilityIdentifier("terminal.menu.browser")
            }
            if session.canLeave || session.canClose {
                Divider()
            }
            if session.canLeave {
                Button("Leave", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) { prompt = .leave(session) }
                    .accessibilityIdentifier("terminal.menu.leave")
            }
            if session.canClose {
                Button("Close terminal", systemImage: "xmark.circle", role: .destructive) { prompt = .closing(session) }
                    .accessibilityIdentifier("terminal.menu.close")
            }
        } label: {
            Label("Terminal actions", systemImage: "ellipsis")
        }
        .accessibilityIdentifier("terminal.menu")
    }
}
