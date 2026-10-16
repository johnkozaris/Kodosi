import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions
import SwiftUI

struct TerminalSessionState: View {
    let session: RuntimeSession
    let opening: Bool
    let failure: String?
    let connect: () -> Void

    var body: some View {
        switch session.connectionState {
        case .blocked:
            TerminalStateCard(symbol: "lock", title: session.statusLabel, message: session.message, working: false) { EmptyView() }
                .accessibilityIdentifier("terminal.state.blocked")
        case .offline where !opening:
            TerminalStateCard(symbol: "wifi.slash", title: session.statusLabel, message: failure ?? session.message, working: false) {
                if session.canOpen {
                    Button("Connect", action: connect)
                        .buttonStyle(.kodosi(.primary))
                        .accessibilityIdentifier("terminal.connect")
                }
            }
            .accessibilityIdentifier("terminal.state.offline")
        default:
            TerminalStateCard(symbol: nil, title: String(localized: "Connecting"), message: nil, working: true) { EmptyView() }
                .accessibilityIdentifier("terminal.state.connecting")
        }
    }
}

struct TerminalRenderOverlay: View {
    @Environment(\.theme) private var theme
    let state: TerminalRenderState
    let retry: () -> Void

    var body: some View {
        switch state {
        case .rendered:
            EmptyView()
        case .connecting:
            TerminalStateCard(symbol: nil, title: String(localized: "Connecting"), message: nil, working: true) { EmptyView() }
                .background(theme.colors.terminal)
                .accessibilityIdentifier("terminal.state.connecting")
        case let .closed(message):
            failure(message ?? String(localized: "The terminal closed."))
        case let .failed(message):
            failure(message)
        }
    }

    private func failure(_ message: String) -> some View {
        TerminalStateCard(symbol: "bolt.slash", title: String(localized: "This terminal is not available"), message: message, working: false) {
            Button("Try again", action: retry)
                .buttonStyle(.kodosi(.primary))
                .accessibilityIdentifier("terminal.retry")
        }
        .background(theme.colors.terminal)
        .accessibilityIdentifier("terminal.state.failed")
    }
}

struct TerminalEnded: View {
    let back: () -> Void

    var body: some View {
        TerminalStateCard(symbol: "moon.zzz", title: String(localized: "This terminal has ended."), message: nil, working: false) {
            Button("Back to terminals", action: back)
                .buttonStyle(.kodosi(.secondary))
                .accessibilityIdentifier("terminal.ended.back")
        }
        .accessibilityIdentifier("terminal.state.ended")
    }
}

struct TerminalSizeNote: View {
    @Environment(\.theme) private var theme
    let note: String
    let resize: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "arrow.up.left.and.arrow.down.right")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.colors.caution)
            Text(note)
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.ink)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            Button("Resize here", action: resize)
                .buttonStyle(.kodosi(.tinted, size: .small))
                .accessibilityIdentifier("terminal.sizeNote.resize")
        }
        .padding(.leading, 14)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .floating)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("terminal.sizeNote")
    }
}
