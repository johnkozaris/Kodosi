import AppKit
import KodosiDesign
import SwiftUI
import UserNotifications

/// The permissions of macOS that Kodosi uses, with their state and the way to give them. macOS
/// tells no app whether it has App Management, so its row only opens the list.
struct PermissionsGroup: View {
    @Environment(\.theme) private var theme
    @State private var notifications: UNAuthorizationStatus?

    var body: some View {
        ListGroup(title: "Permissions", footer: "Programs in your terminals have the permissions that you give Kodosi.") {
            FullDiskAccessRow()
            GrantRow(pane: .accessibility, title: Self.deviceControl,
                     use: String(localized: "For programs in your terminals that control apps"), symbol: "accessibility",
                     tint: TileTint.teal, identifier: "settings.permissions.accessibility") { Privacy.accessibility }
            GrantRow(pane: .screenRecording, title: String(localized: "Screen & System Audio Recording"),
                     use: String(localized: "For programs in your terminals that see the screen"), symbol: "record.circle",
                     tint: TileTint.purple, identifier: "settings.permissions.screenRecording") { Privacy.screenRecording }
            ListRow(String(localized: "App Management"), subtitle: String(localized: "For programs in your terminals that change other apps"),
                    symbol: "app.badge.checkmark.fill", tint: TileTint.blue)
            {
                Button("Open Settings…") { PrivacyGuide.shared.show(.appManagement, theme: theme) }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier("settings.permissions.appManagement")
            }
            ListRow(String(localized: "Notifications"), subtitle: notificationsLine, symbol: "bell.badge.fill", tint: TileTint.red) {
                switch notifications {
                case .notDetermined:
                    Button("Allow") {
                        Task { @MainActor in
                            _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
                            await refresh()
                        }
                    }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier("settings.permissions.notifications")
                case .denied:
                    Button("Open Settings…") { Privacy.openNotifications() }
                        .buttonStyle(.kodosi(.secondary, size: .small))
                        .accessibilityIdentifier("settings.permissions.notifications")
                default:
                    EmptyView()
                }
            }
        }
        .task { await refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await refresh() }
        }
    }

    /// The name of the list of Accessibility in System Settings, which macOS 27 renamed.
    private static var deviceControl: String {
        if #available(macOS 27, *) {
            String(localized: "Device Control and Data Access")
        } else {
            String(localized: "Accessibility")
        }
    }

    private var notificationsLine: String? {
        switch notifications {
        case nil: nil
        case .notDetermined: String(localized: "When a terminal waits or finishes")
        case .denied: String(localized: "Off")
        default: String(localized: "On")
        }
    }

    private func refresh() async {
        notifications = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
    }
}

/// Full Disk Access: on, or a button that opens it in System Settings with Kodosi's icon to drag
/// into the list.
struct FullDiskAccessRow: View {
    @Environment(\.theme) private var theme
    @State private var granted = Privacy.fullDiskAccess

    var body: some View {
        ListRow(String(localized: "Full Disk Access"),
                subtitle: granted ? String(localized: "On") : String(localized: "For the sign-ins of Safari and other browsers"),
                symbol: "externaldrive.fill", tint: TileTint.graphite)
        {
            if !granted {
                Button("Allow…") { PrivacyGuide.shared.show(.fullDiskAccess, theme: theme) }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier("settings.permissions.fullDiskAccess")
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            granted = Privacy.fullDiskAccess
        }
    }
}

/// A permission that Kodosi asks macOS for: on, or a button that asks, which lists Kodosi in
/// System Settings, and opens that list with Kodosi's icon beside it.
private struct GrantRow: View {
    @Environment(\.theme) private var theme
    let pane: Privacy.Pane
    let title: String
    let use: String
    let symbol: String
    let tint: Color
    let identifier: String
    let granted: () -> Bool
    @State private var on = false

    var body: some View {
        ListRow(title, subtitle: on ? String(localized: "On") : use, symbol: symbol, tint: tint) {
            if !on {
                Button("Allow…") { PrivacyGuide.shared.show(pane, theme: theme) }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier(identifier)
            }
        }
        .onAppear { on = granted() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            on = granted()
        }
    }
}
