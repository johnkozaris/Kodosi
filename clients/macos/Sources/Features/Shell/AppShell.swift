import KodosiBridge
import KodosiDesign
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
                    .frame(width: workbench.shownSidebarWidth)
                    .overlay(alignment: .trailing) {
                        if !workbench.sidebarCollapsed {
                            SidebarResizeHandle().offset(x: 4)
                        }
                    }
                    .zIndex(1)
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
            .accessibilityHidden(sheet != nil)
            .overlay(alignment: .top) { notice }
            .disabled(sheet != nil)
            if workbench.showsPalette {
                CommandPalette()
            }
            ZStack {
                if let sheet {
                    sheetLayer(sheet).transition(.opacity)
                }
            }
            .animation(theme.motion.fade, value: sheet)
            .zIndex(1)
        }
        .ignoresSafeArea()
        .kodosiPopoverLayer()
        .buttonStyle(.kodosi(.secondary))
        .textFieldStyle(WellTextFieldStyle()).toggleStyle(KodosiSwitchStyle())
        .onChange(of: workbench.stagedSessionIds) { _, _ in deps.reconcileTerminals() }
        .onChange(of: workbench.focusedSessionId) { _, _ in deps.reconcileTerminals() }
        .onChange(of: workbench.selectedSessionId) { _, _ in deps.reportVisible() }
        .onChange(of: workbench.section) { _, _ in deps.reportVisible() }
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
        sheet == nil && !deps.workbench.showsPalette && deps.workbench.sharingSessionId == nil
    }

    private enum Sheet: Equatable {
        case signIn, recoveryKey, details(String), newRoom, history, launcher
    }

    private var sheet: Sheet? {
        let workbench = deps.workbench
        if deps.signInPresented {
            return .signIn
        }
        if deps.account.newRecoveryKey != nil {
            return .recoveryKey
        }
        if let id = workbench.detailsSessionId, deps.session(id) != nil {
            return .details(id)
        }
        if workbench.showsNewRoom {
            return .newRoom
        }
        if workbench.showsHistory {
            return .history
        }
        return workbench.launcher == nil ? nil : .launcher
    }

    @ViewBuilder
    private func sheetLayer(_ sheet: Sheet) -> some View {
        let workbench = deps.workbench
        switch sheet {
        case .signIn:
            SheetLayer(close: deps.dismissSignIn, cancel: deps.cancelSignIn) { SignInSheet() }
        case .recoveryKey:
            SheetLayer(close: {}, content: { RecoveryKeySheet(key: deps.account.newRecoveryKey ?? "", done: deps.account.dismissRecoveryKey) })
        case let .details(id):
            SheetLayer(close: { workbench.detailsSessionId = nil }, content: {
                if let session = deps.session(id) {
                    SessionDetailsView(session: session)
                }
            })
        case .newRoom:
            SheetLayer(close: { workbench.showsNewRoom = false }, content: { NewRoomSheet() })
        case .history:
            SheetLayer(close: { workbench.showsHistory = false }, content: { HistoryView() })
        case .launcher:
            SheetLayer(close: { workbench.launcher = nil }, content: {
                AgentLauncher(
                    roomId: workbench.launcher?.roomId, defaultFolder: deps.selectedLocalDirectory,
                    started: { workbench.launcher = nil }
                )
                .padding(24)
            })
        }
    }

    private var notice: some View {
        ZStack {
            if deps.updateRequired {
                NoticePill(message: String(localized: "This version of Kodosi is too old for the server. Update Kodosi."),
                           identifier: "shell.update", retry: deps.updater.configured ? { deps.updater.check() } : nil,
                           actionTitle: "Check for Updates", dismiss: { deps.updateRequired = false })
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            } else if let error = deps.errorMessage {
                NoticePill(message: error, identifier: "shell.error") { deps.errorMessage = nil }
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .padding(.leading, deps.workbench.shownSidebarWidth)
        .animation(theme.motion.spring, value: deps.errorMessage)
        .animation(theme.motion.spring, value: deps.updateRequired)
    }
}
