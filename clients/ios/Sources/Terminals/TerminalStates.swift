import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions
import SwiftUI

extension AppModel {
    func unreachable(_ session: RuntimeSession) -> Bool {
        session.kind == .remote && !account.online && session.connectionState != .blocked
    }

    func statusLabel(of session: RuntimeSession) -> String {
        unreachable(session) ? String(localized: "Offline") : session.statusLabel
    }
}

/// Unified placeholder covering both connection and render states so covers never stack.
struct TerminalCover: Equatable {
    enum Action: Equatable {
        case connect, reconnect, retry, wake, waking
    }

    let symbol: String?
    let title: String
    let message: String?
    let working: Bool
    let action: Action?
    let identifier: String

    static let connecting = TerminalCover(symbol: nil, title: String(localized: "Connecting"), message: nil, working: true, action: nil,
                                          identifier: "terminal.state.connecting")

    /// Returns `nil` once the terminal draws; `render` is `nil` until the terminal has a surface.
    static func of(
        _ session: RuntimeSession, unreachable: Bool, opening: Bool, failure: String?, power: ComputerPower?, render: TerminalRenderState?,
    ) -> TerminalCover? {
        switch session.connectionState {
        case .blocked:
            TerminalCover(symbol: "lock", title: session.statusLabel, message: session.message, working: false, action: nil,
                          identifier: "terminal.state.blocked")
        case .connecting where unreachable, .offline where unreachable:
            TerminalCover(symbol: "wifi.slash", title: String(localized: "Offline"),
                          message: String(localized: "The terminal opens when the connection is back."), working: false, action: .reconnect,
                          identifier: "terminal.state.unreachable")
        case .offline where session.status == .offline:
            TerminalCover(symbol: "wifi.slash", title: session.statusLabel,
                          message: power?.problem ?? String(localized: "The terminal opens when its computer is back."),
                          working: power?.busy == .waking, action: power?.busy == .waking ? .waking : power?.canWake == true ? .wake : nil,
                          identifier: "terminal.state.away")
        case .offline where !opening:
            TerminalCover(symbol: "wifi.slash", title: session.statusLabel, message: failure ?? session.message, working: false,
                          action: session.canOpen ? .connect : nil, identifier: "terminal.state.offline")
        case .local, .connected:
            switch render {
            case .rendered: nil
            case let .closed(message): unavailable(message ?? String(localized: "The terminal closed."))
            case let .failed(message): unavailable(message)
            case .connecting, nil: connecting
            }
        case .connecting, .offline:
            connecting
        }
    }

    private static func unavailable(_ message: String) -> TerminalCover {
        TerminalCover(symbol: "bolt.slash", title: String(localized: "This terminal is not available"), message: message, working: false,
                      action: .retry, identifier: "terminal.state.failed")
    }
}

struct TerminalCoverCard: View {
    @Environment(\.theme) private var theme
    let cover: TerminalCover
    let connect: () -> Void
    let reconnect: () -> Void
    let retry: () -> Void
    let wake: () -> Void

    var body: some View {
        TerminalStateCard(symbol: cover.symbol, title: cover.title, message: cover.message, working: cover.working) {
            switch cover.action {
            case .wake, .waking:
                Button(cover.action == .wake ? "Wake" : "Waking…", action: wake)
                    .buttonStyle(.kodosi(.primary))
                    .disabled(cover.action == .waking)
                    .accessibilityIdentifier("terminal.wake")
            case .connect:
                Button("Connect", action: connect)
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("terminal.connect")
            case .reconnect:
                Button("Try again", action: reconnect)
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("terminal.unreachable.retry")
            case .retry:
                Button("Try again", action: retry)
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("terminal.retry")
            case nil:
                EmptyView()
            }
        }
        .background(theme.colors.terminal)
        .accessibilityIdentifier(cover.identifier)
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
    @Environment(\.dynamicTypeSize) private var typeSize
    @ScaledMetric(relativeTo: .footnote) private var glyph: CGFloat = 12
    let note: String
    let resize: () -> Void

    var body: some View {
        let wraps = typeSize.isAccessibilitySize
        let layout = wraps ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8)) : AnyLayout(HStackLayout(spacing: 10))
        layout {
            HStack(spacing: 10) {
                Image(systemName: "arrow.up.left.and.arrow.down.right")
                    .font(.system(size: glyph, weight: .semibold))
                    .foregroundStyle(theme.colors.caution)
                Text(note)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.ink)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !wraps {
                Spacer(minLength: 4)
            }
            Button("Resize here", action: resize)
                .buttonStyle(.kodosi(.tinted, size: .small))
                .accessibilityIdentifier("terminal.sizeNote.resize")
        }
        .padding(.leading, 14)
        .padding(.trailing, wraps ? 14 : 6)
        .padding(.vertical, wraps ? 10 : 6)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .floating)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("terminal.sizeNote")
    }
}
