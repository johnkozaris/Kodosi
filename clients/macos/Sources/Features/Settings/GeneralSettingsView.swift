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

    var body: some View {
        @Bindable var updater = deps.updater
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
            ListGroup(footer: "Agents in Kodosi terminals always find the kodosi command. Install it to use it in other terminals.") {
                ListRow(String(localized: "Command line"),
                        subtitle: deps.commandInstalled ? CommandLineTool.link.path : String(localized: "Not installed"),
                        monoSubtitle: deps.commandInstalled, symbol: "terminal", tint: TileTint.graphite)
                {
                    Button(deps.commandInstalled ? "Installed" : "Install") { deps.installCommand() }
                        .disabled(deps.commandInstalled)
                        .accessibilityIdentifier("settings.command.install")
                }
            }
        }
    }
}
