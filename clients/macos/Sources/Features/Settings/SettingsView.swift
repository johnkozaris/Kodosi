import KodosiDesign
import SwiftUI

struct SettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        @Bindable var workbench = deps.workbench
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                PageHeader(title: "Settings") {
                    SegmentedPill(selection: $workbench.settingsSection, options: [
                        SegmentOption(value: .terminal, title: String(localized: "Terminal"), symbol: "apple.terminal"),
                        SegmentOption(value: .providers, title: String(localized: "Agents"), symbol: "sparkles"),
                        SegmentOption(value: .dosi, title: String(localized: "Dosi"), symbol: "eyes"),
                        SegmentOption(value: .devices, title: String(localized: "Account"), symbol: "person.crop.circle"),
                        SegmentOption(value: .general, title: String(localized: "General"), symbol: "gearshape"),
                    ], identifier: "settings")
                }
                Group {
                    switch workbench.settingsSection {
                    case .providers: ProviderSettingsView()
                    case .dosi: DosiSettingsView()
                    case .devices: DeviceSettingsView()
                    case .terminal: TerminalSettingsView()
                    case .general: GeneralSettingsView()
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
