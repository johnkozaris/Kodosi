import KodosiDesign
import SwiftUI

struct SettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        let workbench = deps.workbench
        let sections = WorkbenchState.SettingsSection.allCases
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                PageHeader(title: "Settings") {
                    SectionDock(
                        items: sections.enumerated().map { index, section in
                            DockItem(id: index, title: section.title, symbol: section.symbol, key: section.rawValue)
                        },
                        selection: sections.firstIndex(of: workbench.settingsSection) ?? 0,
                        identifier: "settings",
                    ) { workbench.settingsSection = sections[$0] }
                }
                Group {
                    switch workbench.settingsSection {
                    case .providers: ProviderSettingsView()
                    case .dosi: DosiSettingsView()
                    case .browser: BrowserSettingsView()
                    case .documents: DocumentSettingsView()
                    case .devices: DeviceSettingsView()
                    case .terminal: TerminalSettingsView()
                    case .general: GeneralSettingsView()
                    case .shortcuts: ShortcutSettingsView()
                    }
                }
                .id(workbench.settingsSection)
                .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: theme.motion.reduced ? 0 : 6)), removal: .identity))
            }
            .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 90)
            .frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}

private extension WorkbenchState.SettingsSection {
    var title: String {
        switch self {
        case .terminal: String(localized: "Terminal")
        case .providers: String(localized: "Agents")
        case .dosi: String(localized: "Dosi")
        case .browser: String(localized: "Browser")
        case .documents: String(localized: "Documents")
        case .devices: String(localized: "Account")
        case .general: String(localized: "General")
        case .shortcuts: String(localized: "Shortcuts")
        }
    }

    var symbol: String {
        switch self {
        case .terminal: "apple.terminal"
        case .providers: "sparkles"
        case .dosi: "eyes"
        case .browser: "globe"
        case .documents: "doc.text"
        case .devices: "person.crop.circle"
        case .general: "gearshape"
        case .shortcuts: "command"
        }
    }
}
