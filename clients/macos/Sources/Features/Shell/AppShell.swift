import SwiftUI

struct AppShell: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        @Bindable var workbench = deps.workbench
        ZStack {
            GroundBackdrop()
            HStack(spacing: 0) {
                SidebarView()
                    .frame(width: workbench.sidebarCollapsed ? Metrics.sidebarRailWidth : Metrics.sidebarWidth)
                destination
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background {
                        RaisedBackground(
                            shape: RoundedRectangle(cornerRadius: Radius.xl, style: .continuous),
                            fill: theme.colors.surface, elevation: .resting
                        )
                    }
                    .padding([.top, .trailing, .bottom], Metrics.frameInset)
            }
            .overlay(alignment: .top) { notice }
            if workbench.showsPalette {
                CommandPalette().transition(.opacity)
            }
        }
        .ignoresSafeArea()
        .buttonStyle(.kodosi(.secondary))
        .textFieldStyle(WellTextFieldStyle()).toggleStyle(SwitchToggleStyle())
        .animation(theme.motion.spring, value: deps.errorMessage)
        .animation(theme.motion.fade, value: workbench.showsPalette)
        .sheet(isPresented: $workbench.showsHistory) { HistoryView() }
        .sheet(isPresented: $workbench.showsNewRoom) { NewRoomSheet() }
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
        .onChange(of: workbench.selectedSessionId) { _, id in
            if let id {
                deps.attention.remove(id)
            }
        }
    }

    @ViewBuilder
    private var destination: some View {
        let workbench = deps.workbench
        switch workbench.section {
        case .sessions:
            TilingStage(workbench: workbench)
                .environment(\.terminalInteractionEnabled, terminalsInteractive)
        case .missions: MissionsView()
        case .people: PeopleView()
        case .settings: SettingsView()
        }
    }

    private var terminalsInteractive: Bool {
        let workbench = deps.workbench
        return !workbench.showsHistory && !workbench.showsPalette && !workbench.showsNewRoom
            && workbench.detailsSessionId == nil && workbench.sharingSessionId == nil
    }

    @ViewBuilder
    private var notice: some View {
        if let error = deps.errorMessage {
            NoticePill(message: error, identifier: "shell.error") { deps.errorMessage = nil }
                .padding(.top, Metrics.frameInset + 10)
                .padding(.leading, deps.workbench.sidebarCollapsed ? Metrics.sidebarRailWidth : Metrics.sidebarWidth)
                .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}
