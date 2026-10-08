import SwiftUI

struct SettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                settingsButton("Terminal", id: .terminal, symbol: "terminal")
                settingsButton("Providers", id: .providers, symbol: "slider.horizontal.3")
                settingsButton("Account & Devices", id: .devices, symbol: "desktopcomputer")
                Spacer()
            }.padding(12).frame(width: 210).background(theme.colors.surfacePanel).seamBorder(.trailing)
            ScrollView {
                Group {
                    switch deps.workbench.settingsSection {
                    case .providers: ProviderSettingsView()
                    case .devices: DeviceSettingsView()
                    default: TerminalSettingsView()
                    }
                }
                .padding(28).frame(maxWidth: 780, alignment: .leading).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private func settingsButton(_ title: LocalizedStringKey, id: WorkbenchState.SettingsSection, symbol: String) -> some View {
        Button { deps.workbench.settingsSection = id } label: {
            Label(title, systemImage: symbol).appTextStyle(.button)
                .frame(maxWidth: .infinity, alignment: .leading).padding(10)
                .foregroundStyle(deps.workbench.settingsSection == id ? theme.colors.primary : theme.colors.foreground)
                .background(deps.workbench.settingsSection == id ? theme.colors.secondary : .clear)
        }.buttonStyle(.plain).accessibilityIdentifier("settings.\(id)")
    }
}
