import KodosiBridge
import SwiftUI

/// The one state of a canvas video, over its picture: nothing while it is live, a dot while the
/// last picture rests and a word once it rests long, a spinner when nothing showed yet, one line
/// while it is away, and Try again once something broke.
struct VideoCover: View {
    static let reconnecting = Duration.seconds(6)
    static let opening = Duration.milliseconds(500)

    let showing: VideoShowing
    let computer: String
    /// Says what does not show, once something broke.
    let failed: String
    /// What broke, which only assistive technologies say.
    let failure: String?
    let identifier: String
    let retry: () -> Void

    var body: some View {
        switch showing {
        case let .away(away):
            TerminalStateCard(symbol: away.symbol, title: away.line(on: computer), message: nil, working: false) {
                EmptyView()
            }
            .accessibilityIdentifier("\(identifier).away")
        case .failed:
            TerminalStateCard(symbol: "exclamationmark.triangle", title: failed, message: nil, working: false) {
                Button(action: retry) { Text("Try again", bundle: .module) }
                    .buttonStyle(.kodosi(.secondary))
                    .accessibilityHint(Text(verbatim: failure ?? ""))
                    .accessibilityIdentifier("\(identifier).retry")
            }
            .accessibilityIdentifier("\(identifier).failed")
        case .live:
            EmptyView()
        case .resting:
            RestingMark(identifier: identifier)
        case .opening:
            OpeningSpinner()
                .accessibilityIdentifier("\(identifier).opening")
        }
    }
}

private struct RestingMark: View {
    @Environment(\.theme) private var theme
    let identifier: String
    @State private var long = false

    var body: some View {
        ZStack {
            if long {
                ReconnectingNotice(identifier: "\(identifier).reconnecting")
            } else {
                BreathingDot(color: theme.colors.glowOrange)
                    .frame(height: 34)
                    .transition(.opacity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .padding(.top, 10)
        .allowsHitTesting(false)
        .animation(theme.motion.spring, value: long)
        .task {
            try? await Task.sleep(for: VideoCover.reconnecting)
            if !Task.isCancelled {
                long = true
            }
        }
    }
}

private struct OpeningSpinner: View {
    @State private var shows = false

    var body: some View {
        ZStack {
            if shows {
                Spinner(size: 18).transition(.opacity)
            }
        }
        .task {
            try? await Task.sleep(for: VideoCover.opening)
            if !Task.isCancelled {
                shows = true
            }
        }
    }
}

extension VideoShowing {
    /// Away and failed say something in place of the picture.
    var speaks: Bool {
        switch self {
        case .away, .failed: true
        case .live, .resting, .opening: false
        }
    }
}

extension VideoAway {
    var symbol: String {
        switch self {
        case .sharingOff: "eye.slash"
        case .offline: "wifi.slash"
        case .locked: "lock"
        case .anotherAccount: "person.2"
        case .asleep: "moon"
        case .unrecorded: "record.circle"
        }
    }

    func line(on computer: String) -> String {
        switch self {
        case .sharingOff: String(localized: "Sharing is off on \(computer)", bundle: .module)
        case .offline: String(localized: "\(computer) is offline", bundle: .module)
        case .locked: String(localized: "\(computer) is locked", bundle: .module)
        case .anotherAccount: String(localized: "\(computer) is on another account", bundle: .module)
        case .asleep: String(localized: "\(computer) is asleep", bundle: .module)
        case .unrecorded: String(localized: "Allow screen recording on \(computer)", bundle: .module)
        }
    }
}

/// A check at the bottom of the picture for a moment, once text copied there is on this device.
struct CopiedNotice: View {
    private static let stays = Duration.milliseconds(1500)

    @Environment(\.theme) private var theme
    let copies: Int
    @State private var shown = false

    var body: some View {
        ZStack {
            if shown {
                HStack(spacing: 6) {
                    Image(systemName: "checkmark").font(scaled: 10, weight: .bold).foregroundStyle(theme.colors.ready)
                    Text("Copied", bundle: .module).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                }
                .padding(.leading, 11).padding(.trailing, 14).controlHeight(34)
                .raisedCapsule(fill: theme.colors.lifted, elevation: .floating)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("canvas.copied")
                .transition(.opacity.combined(with: .offset(y: 8)))
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        .padding(.bottom, 12)
        .allowsHitTesting(false)
        .animation(theme.motion.spring, value: shown)
        .task(id: copies) {
            guard copies > 0 else { return }
            shown = true
            AccessibilityNotification.Announcement(String(localized: "Copied", bundle: .module)).post()
            try? await Task.sleep(for: Self.stays)
            if !Task.isCancelled {
                shown = false
            }
        }
        #if os(iOS)
        .sensoryFeedback(.success, trigger: copies)
        #endif
    }
}
