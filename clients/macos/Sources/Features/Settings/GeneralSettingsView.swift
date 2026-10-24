import KodosiDesign
import SwiftUI

struct GeneralSettingsView: View {
    @Environment(AppDependencies.self) private var deps

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
    }

    private var updatesFooter: LocalizedStringKey {
        if !deps.updater.configured {
            return "This build of Kodosi does not update itself."
        }
        return deps.updater.automatic
            ? "Kodosi downloads updates in the background and installs them when you quit it."
            : "Kodosi asks before it downloads an update."
    }

    private var closeFooter: LocalizedStringKey {
        deps.settings.keepRunning
            ? "Your terminals keep running, and Kodosi stays in the menu bar."
            : "Closing the window quits Kodosi. Your terminals stop, and start again when Kodosi opens."
    }

    private var commandFooter: LocalizedStringKey {
        guard let link = deps.commandLink else {
            return "Agents in Kodosi terminals always find the kodosi command."
        }
        if deps.launcher.catalog != nil, deps.launcher.catalog?.kodosi == nil {
            return "Add \((link as NSString).deletingLastPathComponent) to PATH in your shell to use kodosi in other terminals."
        }
        return "Agents in Kodosi terminals and in other terminals find the kodosi command."
    }

    var body: some View {
        @Bindable var updater = deps.updater
        @Bindable var settings = deps.settings
        VStack(alignment: .leading, spacing: 22) {
            ListGroup(footer: updatesFooter) {
                ListRow(String(localized: "Version"), subtitle: version, symbol: "app.badge", tint: TileTint.blue) {
                    Button("Check for Updates…") { updater.check() }
                        .disabled(!updater.configured)
                        .accessibilityIdentifier("settings.updates.check")
                }
                Toggle(isOn: $updater.automatic) {
                    HStack(spacing: 12) {
                        IconTile(symbol: "arrow.down.circle", tint: TileTint.green, size: 28)
                        Text("Update automatically")
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 52)
                .disabled(!updater.configured)
                .accessibilityIdentifier("settings.updates.automatic")
            }
            ListGroup(footer: closeFooter) {
                Toggle(isOn: $settings.keepRunning) {
                    HStack(spacing: 12) {
                        IconTile(symbol: "macwindow", tint: TileTint.blue, size: 28)
                        Text("Keep running when the window closes")
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 52)
                .accessibilityIdentifier("settings.keepRunning")
            }
            ListGroup(footer: commandFooter) {
                ListRow(String(localized: "Command line"),
                        subtitle: deps.commandLink ?? String(localized: "Only in Kodosi terminals"),
                        monoSubtitle: deps.commandLink != nil, symbol: "terminal", tint: TileTint.graphite)
            }
        }
    }
}
