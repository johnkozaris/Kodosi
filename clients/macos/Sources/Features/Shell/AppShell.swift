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
            .disabled(sheet != nil)
            if workbench.showsPalette {
                CommandPalette().transition(.opacity)
            }
            if let sheet {
                sheetLayer(sheet).transition(.opacity).zIndex(1)
            }
        }
        .ignoresSafeArea()
        .kodosiPopoverLayer()
        .buttonStyle(.kodosi(.secondary))
        .textFieldStyle(WellTextFieldStyle()).toggleStyle(SwitchToggleStyle())
        .animation(theme.motion.spring, value: deps.errorMessage)
        .animation(theme.motion.fade, value: workbench.showsPalette)
        .animation(theme.motion.fade, value: sheet)
        .onChange(of: workbench.stagedSessionIds) { _, _ in deps.reconcileTerminals() }
        .onChange(of: workbench.focusedSessionId) { _, _ in deps.reconcileTerminals() }
        .onChange(of: workbench.selectedSessionId) { _, id in
            if let id {
                deps.seen(id)
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
        sheet == nil && !deps.workbench.showsPalette && deps.workbench.sharingSessionId == nil
    }

    /// The sheet that shows over the window, if any; at most one shows at a time.
    private enum Sheet: Equatable {
        case signIn, details(String), newRoom, history
    }

    private var sheet: Sheet? {
        let workbench = deps.workbench
        if deps.signInPresented {
            return .signIn
        }
        if let id = workbench.detailsSessionId, deps.session(id) != nil {
            return .details(id)
        }
        if workbench.showsNewRoom {
            return .newRoom
        }
        return workbench.showsHistory ? .history : nil
    }

    @ViewBuilder
    private func sheetLayer(_ sheet: Sheet) -> some View {
        let workbench = deps.workbench
        switch sheet {
        case .signIn:
            SheetLayer(close: deps.dismissSignIn, cancel: deps.cancelSignIn) { SignInSheet() }
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
        }
    }

    private var notice: some View {
        Group {
            if deps.updateRequired {
                NoticePill(message: String(localized: "This version of Kodosi is too old for the server. Update Kodosi."),
                           identifier: "shell.update", retry: deps.updater.configured ? { deps.updater.check() } : nil,
                           actionTitle: "Check for Updates", dismiss: { deps.updateRequired = false })
            } else if let error = deps.errorMessage {
                NoticePill(message: error, identifier: "shell.error") { deps.errorMessage = nil }
            } else if deps.offersCommand {
                NoticePill(message: String(localized: "Use the kodosi command in all your terminals?"),
                           identifier: "shell.command", retry: { deps.installCommand() },
                           actionTitle: "Install", dismiss: { deps.offersCommand = false })
            }
        }
        .padding(.top, Metrics.frameInset + 10)
        .padding(.leading, deps.workbench.sidebarCollapsed ? Metrics.sidebarRailWidth : Metrics.sidebarWidth)
        .transition(.move(edge: .top).combined(with: .opacity))
    }
}
