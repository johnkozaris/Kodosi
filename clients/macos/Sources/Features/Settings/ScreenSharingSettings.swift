import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// This Mac's screen on the canvas of its terminals, for its owner's other devices. Kodosi Screen
/// records and controls it with permissions of its own, which the programs in the terminals do
/// not get.
struct ScreenSharingGroup: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var sharing: ScreenSharing?
    @State private var problem: String?

    private var footer: LocalizedStringKey {
        guard sharing?.allowed == true else {
            return "Your phone and your other computers can show this Mac's screen on the canvas of its terminals."
        }
        return """
        Kodosi Screen, a part of Kodosi, records and controls this Mac for them, so the programs in your terminals do not get \
        these permissions. While someone watches, the menu bar says who, with a button to stop.
        """
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let sharing, sharing.available {
                ListGroup(title: "Screen", footer: footer) {
                    SwitchRow(String(localized: "Let my other devices see and control this Mac"), symbol: CanvasSymbol.screen,
                              tint: TileTint.blue, isOn: Binding { sharing.allowed } set: { allow($0) })
                        .accessibilityIdentifier("settings.screen.share")
                    if sharing.allowed {
                        ScreenGrantRow(pane: .screenRecording, title: String(localized: "Screen & System Audio Recording"),
                                       use: String(localized: "For Kodosi Screen to see this Mac"), symbol: "record.circle",
                                       tint: TileTint.purple, granted: sharing.access?.screen,
                                       identifier: "settings.screen.screenRecording")
                        ScreenGrantRow(pane: .accessibility, title: PermissionsGroup.deviceControl,
                                       use: String(localized: "For Kodosi Screen to control this Mac"), symbol: "accessibility",
                                       tint: TileTint.teal, granted: sharing.access?.input, identifier: "settings.screen.accessibility")
                    }
                }
            }
            if let problem {
                ErrorNote(message: problem)
            }
        }
        .animation(theme.motion.spring, value: sharing?.allowed)
        .task { await refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await refresh() }
        }
    }

    private func refresh() async {
        do {
            sharing = try await deps.commandSink.screenSharing()
        } catch {
            problem = error.localizedDescription
        }
    }

    private func allow(_ allowed: Bool) {
        Task {
            do {
                sharing = try await deps.commandSink.shareScreen(allowed)
                problem = nil
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

private struct ScreenGrantRow: View {
    @Environment(\.theme) private var theme
    let pane: Privacy.Pane
    let title: String
    let use: String
    let symbol: String
    let tint: Color
    /// `nil` while Kodosi Screen has not said.
    let granted: Bool?
    let identifier: String

    var body: some View {
        ListRow(title, subtitle: granted == true ? String(localized: "On") : use, symbol: symbol, tint: tint) {
            if granted == false {
                Button("Allow…") { PrivacyGuide.shared.show(pane, for: .screen, theme: theme) }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier(identifier)
            }
        }
    }
}
