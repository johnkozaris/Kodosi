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
                            fill: theme.colors.surface, elevation: .resting,
                        )
                    }
                    .padding([.top, .trailing, .bottom], Metrics.frameInset)
                    .background(alignment: .top) { Color.clear.frame(height: Metrics.frameInset).titleBarGap() }
            }
            .accessibilityHidden(sheet != nil)
            .overlay(alignment: .top) { notice }
            .disabled(sheet != nil)
            if workbench.showsPalette {
                CommandPalette()
            }
            ZStack {
                if let sheet, sheet != .launcher {
                    sheetLayer(sheet).transition(.opacity)
                }
            }
            .animation(theme.motion.fade, value: sheet)
            .zIndex(1)
            if sheet == .launcher {
                sheetLayer(.launcher).zIndex(1)
            }
        }
        .ignoresSafeArea()
        .kodosiPopoverLayer()
        .buttonStyle(.kodosi(.secondary))
        .textFieldStyle(WellTextFieldStyle()).toggleStyle(KodosiSwitchStyle())
        .confirmationDialog(
            projectShareTitle, isPresented: Binding(get: { workbench.projectShare != nil }, set: {
                if !$0 {
                    workbench.projectShare = nil
                }
            }), titleVisibility: .visible, presenting: workbench.projectShare,
        ) { share in
            Button("Share") { deps.share(share) }
            Button("Cancel", role: .cancel) {}
        } message: { share in
            Text(projectShareMessage(share))
        }
        .onChange(of: workbench.stagedSessionIds) { _, _ in deps.reconcileTerminals() }
        .onChange(of: workbench.focusedSessionId) { _, _ in deps.reconcileTerminals() }
        .background(VisibleReporter())
        .onChange(of: terminalsInteractive, initial: true) { _, open in workbench.stageTakesKeys = open }
    }

    @ViewBuilder
    private var destination: some View {
        let workbench = deps.workbench
        switch workbench.section {
        case .sessions:
            TilingStage(workbench: workbench)
                .environment(\.terminalInteractionEnabled, terminalsInteractive)
        case .projects: ProjectsView()
        case .people: PeopleView()
        case .settings: SettingsView()
        }
    }

    private var projectShareTitle: String {
        guard let share = deps.workbench.projectShare else { return "" }
        let terminal = deps.session(share.sessionId)?.name ?? String(localized: "this terminal")
        return String(localized: "Share \(terminal) with \(projectName(share.projectId))?")
    }

    private func projectShareMessage(_ share: ProjectShare) -> String {
        let project = projectName(share.projectId)
        let control = String(localized: "Everyone in \(project) gets full control of it.")
        guard let other = deps.session(share.sessionId)?.projectId else { return control }
        return control + " " + String(localized: "It leaves \(projectName(other)).")
    }

    private func projectName(_ id: String) -> String {
        deps.projects.entries.first { $0.id == id }?.name ?? String(localized: "the project")
    }

    private var terminalsInteractive: Bool {
        sheet == nil && !deps.workbench.showsPalette && deps.workbench.sharingSessionId == nil
    }

    private enum Sheet: Equatable {
        case signIn, recoveryKey, deviceOffer, trust(FriendTrust), report(ReportTarget), details(String), newProject, history, launcher
    }

    private var sheet: Sheet? {
        let workbench = deps.workbench
        if deps.signInPresented {
            return .signIn
        }
        if deps.account.newRecoveryKey != nil {
            return .recoveryKey
        }
        if deps.account.offer != nil {
            return .deviceOffer
        }
        if let trust = workbench.trusting {
            return .trust(trust)
        }
        if let person = workbench.reporting {
            return .report(person)
        }
        if let id = workbench.detailsSessionId, deps.session(id) != nil {
            return .details(id)
        }
        if workbench.showsNewProject {
            return .newProject
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
        case .deviceOffer:
            SheetLayer(close: deps.account.endOffer, content: { DeviceOfferSheet() })
        case let .trust(trust):
            SheetLayer(close: { workbench.trusting = nil }, content: { FriendTrustSheet(trust: trust) })
        case let .report(person):
            SheetLayer(close: { workbench.reporting = nil }, content: { ReportSheet(person: person) })
        case let .details(id):
            SheetLayer(close: { workbench.detailsSessionId = nil }, content: {
                if let session = deps.session(id) {
                    SessionDetailsView(session: session)
                }
            })
        case .newProject:
            SheetLayer(close: { workbench.showsNewProject = false }, content: { NewProjectSheet() })
        case .history:
            SheetLayer(close: { workbench.showsHistory = false }, content: { HistoryView() })
        case .launcher:
            SheetLayer(close: { workbench.launcher = nil }, rises: false, content: {
                AgentLauncher(
                    projectId: workbench.launcher?.projectId, defaultFolder: deps.selectedLocalDirectory,
                    started: { workbench.launcher = nil }, close: { workbench.launcher = nil },
                )
                .padding(24)
            })
        }
    }

    private var notice: some View {
        ZStack {
            if deps.updateRequired, !deps.updateNoticeDismissed {
                NoticePill(message: String(localized: "This version of Kodosi is too old for the server. Update Kodosi."),
                           identifier: "shell.update", action: deps.updater.started ? { deps.updater.restart() } : nil,
                           actionTitle: Text(verbatim: deps.updater.actionTitle), dismiss: { deps.updateNoticeDismissed = true })
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            } else if let done = deps.doneMessage {
                NoticePill(message: done, identifier: "shell.done", sign: .status(.done)) { deps.doneMessage = nil }
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .task(id: done) {
                        AccessibilityNotification.Announcement(done).post()
                        try? await Task.sleep(for: .seconds(6))
                        if deps.doneMessage == done {
                            deps.doneMessage = nil
                        }
                    }
            } else if let error = deps.errorMessage {
                NoticePill(message: error, identifier: "shell.error") { deps.errorMessage = nil }
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            } else if let device = deps.deviceNotice {
                NoticePill(message: device.message, identifier: "shell.devices", sign: .status(.hand), action: deps.openDeviceNotice,
                           actionTitle: Text(device.actionTitle), dismiss: deps.dismissDeviceNotice)
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .padding(.leading, deps.workbench.shownSidebarWidth)
        .animation(theme.motion.spring, value: deps.errorMessage)
        .animation(theme.motion.spring, value: deps.doneMessage)
        .animation(theme.motion.spring, value: deps.updateRequired && !deps.updateNoticeDismissed)
        .animation(theme.motion.spring, value: deps.deviceNotice)
    }
}

private extension DeviceNotice {
    var message: String {
        switch self {
        case let .reset(reset):
            String(localized: "\(reset.deviceLabel) asked to start fresh. Stop it unless you asked for it.")
        case let .requests(requests) where requests.count == 1:
            String(localized: "\(requests[0].deviceLabel) wants to use your account.")
        case let .requests(requests):
            String(localized: "\(requests.count) devices want to use your account.")
        case .approvalNeeded:
            String(localized: "This Mac needs approval before it can share.")
        case .approvalWaiting:
            String(localized: "This Mac waits for approval from another of your devices.")
        }
    }

    var actionTitle: LocalizedStringKey {
        switch self {
        case .reset: "Review…"
        case .requests: "Approve…"
        case .approvalNeeded: "Open Settings"
        case .approvalWaiting: "Show code"
        }
    }
}

private struct VisibleReporter: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        Color.clear
            .onChange(of: deps.shownSessionIds, initial: true) { _, _ in deps.reportVisible() }
            .onChange(of: deps.frontSessionId) { _, _ in deps.followCanvas() }
    }
}
