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
                        SegmentOption(value: .devices, title: String(localized: "Account"), symbol: "person.crop.circle"),
                    ], identifier: "settings")
                }
                Group {
                    switch workbench.settingsSection {
                    case .providers: ProviderSettingsView()
                    case .devices: DeviceSettingsView()
                    case .terminal: TerminalSettingsView()
                    }
                }
                .id(workbench.settingsSection)
                .transition(.opacity.combined(with: .offset(y: theme.motion.reduced ? 0 : 6)))
            }
            .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 90)
            .frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}
