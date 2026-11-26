import KodosiBridge
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
            ? "Kodosi downloads updates and installs them while nobody uses it."
            : "Kodosi asks before it downloads an update."
    }

    private var closeFooter: LocalizedStringKey {
        deps.settings.keepRunning
            ? "Your terminals keep running, and Kodosi stays in the menu bar."
            : "Closing the window quits Kodosi. Your terminals stop, and start again when Kodosi opens."
    }

    private var awakeFooter: LocalizedStringKey {
        switch deps.power.keepsAwake {
        case .always?: "This Mac does not go to sleep by itself while Kodosi runs. On battery, it stays awake only while agents work."
        case .never?: "This Mac can go to sleep while an agent works. The agent then pauses until the Mac wakes."
        case .agents?, nil:
            "This Mac does not go to sleep by itself while an agent works, or for a few minutes after. An agent that waits for you does not keep it awake."
        }
    }

    private var canvasFooter: LocalizedStringKey {
        deps.settings.opensCanvas
            ? "The canvas opens when the agent shows something on it. A change of a file or the browser on it opens it too, unless you closed it."
            : "The canvas opens only when you open it. Its keys show what is new."
    }

    private var languageFooter: LocalizedStringKey {
        let next = deps.settings.nextLanguage
        guard next != AppLanguage.current else {
            return "Kodosi shows the language of macOS, or the one that you choose here."
        }
        return "Quit Kodosi and open it again to show \(AppLanguage.name(of: next))."
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
                        .disabled(!updater.started)
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
            ListGroup {
                AboutLinks()
            }
            ListGroup(footer: languageFooter) {
                ListRow(String(localized: "Language"), symbol: "globe", tint: TileTint.teal) {
                    MenuPicker(
                        title: "Language",
                        selection: $settings.language,
                        values: [nil] + AppLanguage.choices.map(\.tag),
                        label: { tag in tag.map(AppLanguage.name(of:)) ?? String(localized: "Same as macOS") },
                        identifier: "settings.language",
                    )
                }
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
            if let keepsAwake = deps.power.keepsAwake {
                ListGroup(footer: awakeFooter) {
                    ListRow(String(localized: "Stay awake"), symbol: "sun.max", tint: TileTint.orange) {
                        SegmentedPill(selection: Binding(get: { keepsAwake }, set: { deps.power.keepAwake($0) }), options: [
                            SegmentOption(value: KeepAwake.agents, title: String(localized: "While agents work")),
                            SegmentOption(value: KeepAwake.always, title: String(localized: "Always")),
                            SegmentOption(value: KeepAwake.never, title: String(localized: "Never")),
                        ], compact: true, identifier: "settings.keepAwake")
                    }
                }
            }
            ListGroup(footer: canvasFooter) {
                Toggle(isOn: $settings.opensCanvas) {
                    HStack(spacing: 12) {
                        IconTile(symbol: CanvasSymbol.canvas, tint: TileTint.orange, size: 28)
                        Text("Open the canvas by itself")
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 52)
                .accessibilityIdentifier("settings.opensCanvas")
            }
            ListGroup(footer: commandFooter) {
                ListRow(String(localized: "Command line"),
                        subtitle: deps.commandLink ?? String(localized: "Only in Kodosi terminals"),
                        monoSubtitle: deps.commandLink != nil, symbol: "terminal", tint: TileTint.graphite)
            }
            ScreenSharingGroup()
            PermissionsGroup()
        }
    }
}
