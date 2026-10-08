import SwiftUI

struct AppShell: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Namespace private var navigation
    @State private var errorDetails = false

    var body: some View {
        @Bindable var workbench = deps.workbench
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(theme.isDark ? "KodosiLogoDark" : "KodosiLogo")
                    .resizable().scaledToFit().frame(width: 30, height: 30)
                    .accessibilityLabel(Text("Kodosi"))
                ForEach([WorkbenchState.Section.sessions, .missions, .people]) { section in
                    Button { withAnimation(reduceMotion ? nil : theme.motion.selection) { workbench.section = section } } label: {
                        Label(section.label, systemImage: section.symbol)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(workbench.section == section ? theme.colors.primary : theme.colors.mutedForeground)
                            .padding(.horizontal, 12).padding(.vertical, 8)
                            .background {
                                if workbench.section == section {
                                    RoundedRectangle(cornerRadius: 9).fill(theme.colors.secondary).matchedGeometryEffect(
                                        id: "navigation",
                                        in: navigation
                                    )
                                }
                            }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("header.\(section.rawValue)")
                    .accessibilityAddTraits(workbench.section == section ? .isSelected : [])
                }
                Spacer()
                if let prompt = deps.signInPrompt {
                    Button { deps.beginSignIn() }
                        label: { Text(prompt).frame(height: 20) }
                        .buttonStyle(SolidPrimaryButtonStyle())
                        .accessibilityIdentifier("header.signIn")
                }
                Button {
                    workbench.section = .settings
                } label: {
                    Image(systemName: "gearshape").font(.system(size: 17, weight: .medium)).frame(width: 20, height: 20)
                }
                .buttonStyle(SolidSecondaryButtonStyle())
                .help("Settings").accessibilityLabel(Text("Settings"))
                .accessibilityIdentifier("header.settings")
            }
            .padding(.horizontal, 16).padding(.vertical, 10)
            .background(theme.colors.surfacePanel).seamBorder(.bottom)
            if let error = deps.errorMessage {
                HStack(spacing: 10) {
                    Image(systemName: "exclamationmark.triangle").accessibilityHidden(true)
                    Button("Action failed") { errorDetails = true }.buttonStyle(.plain)
                        .popover(isPresented: $errorDetails) { Text(error).textSelection(.enabled).padding(18).frame(maxWidth: 380) }
                    Spacer()
                    Button { deps.errorMessage = nil } label: { Image(systemName: "xmark") }
                        .buttonStyle(.plain).accessibilityLabel(Text("Dismiss error"))
                        .accessibilityIdentifier("shell.error.dismiss")
                }
                .foregroundStyle(theme.colors.destructive)
                .padding(12).background(theme.colors.surfacePanel)
                .accessibilityIdentifier("shell.error")
            }
            ZStack {
                switch workbench.section {
                case .sessions:
                    HStack(spacing: 0) {
                        SessionSidebarView().frame(width: workbench.sidebarCollapsed ? 44 : workbench.sidebarWidth)
                        TilingStage(workbench: workbench)
                            .environment(
                                \.terminalInteractionEnabled,
                                !workbench.showsHistory && workbench.detailsSessionId == nil && workbench.sharingSessionId == nil
                            )
                    }
                case .missions: MissionsView()
                case .people: PeopleView()
                case .settings: SettingsView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .buttonStyle(SolidSecondaryButtonStyle())
        .textFieldStyle(KodosiTextFieldStyle()).toggleStyle(KodosiToggleStyle())
        .background(theme.colors.background)
        .sheet(isPresented: $workbench.showsHistory) { HistoryView() }
        .sheet(isPresented: Binding(get: { deps.signInPresented }, set: {
            if !$0 {
                deps.dismissSignIn()
            }
        })) {
            SignInSheet().environment(deps)
        }
        .sheet(isPresented: Binding(
            get: { workbench.detailsSessionId != nil },
            set: {
                if !$0 {
                    workbench.detailsSessionId = nil
                }
            }
        )) {
            if let id = workbench.detailsSessionId, let session = deps.session(id) {
                SessionDetailsView(session: session)
            }
        }
        .onChange(of: workbench.stagedSessionIds) { _, _ in deps.reconcileTerminals() }
        .onChange(of: workbench.focusedSessionId) { _, _ in deps.reconcileTerminals() }
    }
}
