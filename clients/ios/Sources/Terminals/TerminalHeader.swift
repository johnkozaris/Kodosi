import KodosiBridge
import KodosiDesign
import SwiftUI

/// The title of a terminal in its bar: its mark, its name with the sign of what it needs, and
/// where it runs or what it does. The title of your own terminal is a key that renames it.
struct TerminalTitle: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let sign: StatusSign.Form?
    let rename: (() -> Void)?

    var body: some View {
        if let rename {
            Button(action: rename) { label.frame(minHeight: 44).contentShape(Rectangle()) }
                .buttonStyle(PressScaleStyle(scale: 0.97))
                .accessibilityHint(Text("Renames the terminal"))
                .accessibilityIdentifier("terminal.header")
        } else {
            label
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("terminal.header")
        }
    }

    private var label: some View {
        HStack(spacing: 9) {
            AgentMark(session: session, size: 22, activity: session.mark(rested: .awake))
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 5) {
                    if rename != nil {
                        GhostKeyFace { name }
                    } else {
                        name
                    }
                    if let sign {
                        StatusSign(form: sign).transition(AnyTransition.pop)
                    }
                }
                caption
                    .appTextStyle(.caption)
                    .lineLimit(1)
            }
        }
        .animation(theme.motion.soft, value: session.name)
        .animation(theme.motion.snappy, value: sign)
    }

    private var name: some View {
        Text(session.name)
            .appTextStyle(.subhead)
            .foregroundStyle(theme.colors.ink)
            .lineLimit(1)
            .contentTransition(.interpolate)
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
            SessionActivityText(session: session)
                .foregroundStyle(theme.colors.inkMuted)
        } else if let place = session.place {
            TerminalLocation(host: session.kind == .remote ? session.hostLabel : nil, glyph: PlaceGlyph.symbol(for: place.kind), name: place.name)
        } else {
            TerminalLocation(host: session.hostLabel, glyph: "folder", name: session.folderName)
        }
    }
}

/// The actions of a terminal, in the system's menu: copper sliders in its bar, or a key cap in
/// the key row.
struct TerminalMenu: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    @Binding var prompt: TerminalPrompt?
    @Binding var showsSharing: Bool
    var capped = false
    let interrupt: () -> Void
    let openBrowser: () -> Void
    let addFile: () -> Void

    var body: some View {
        Menu {
            if session.isOwner {
                Button("Rename", systemImage: "pencil") { prompt = .rename(session) }
                    .accessibilityIdentifier("terminal.menu.rename")
                Button("Share with a room", systemImage: "person.badge.plus") { showsSharing = true }
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
                Button("Add a File…", systemImage: CanvasSymbol.addFile, action: addFile)
                    .accessibilityIdentifier("terminal.menu.addFile")
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
            if capped {
                Image(systemName: "slider.horizontal.3")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(theme.colors.accentStrong)
                    .modifier(TerminalCapFace(highlighted: false))
            } else {
                ActionsKey(lit: false).touchTarget()
            }
        }
        .menuStyle(.button)
        .buttonStyle(PressScaleStyle(scale: 0.9))
        .accessibilityLabel(Text("Terminal actions"))
        .accessibilityIdentifier("terminal.menu")
    }
}
