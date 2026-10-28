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
        .confirmationDialog(
            roomShareTitle, isPresented: Binding(get: { workbench.roomShare != nil }, set: {
                if !$0 {
                    workbench.roomShare = nil
                }
            }), titleVisibility: .visible, presenting: workbench.roomShare,
        ) { share in
            Button("Share") { deps.share(share) }
            Button("Cancel", role: .cancel) {}
        } message: { share in
            Text(roomShareMessage(share))
        }
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

    private var roomShareTitle: String {
        guard let share = deps.workbench.roomShare else { return "" }
        let terminal = deps.session(share.sessionId)?.name ?? String(localized: "this terminal")
        return String(localized: "Share \(terminal) with \(roomName(share.roomId))?")
    }

    private func roomShareMessage(_ share: RoomShare) -> String {
        let room = roomName(share.roomId)
        let control = String(localized: "Everyone in \(room) gets full control of it.")
        guard let other = deps.session(share.sessionId)?.missionId else { return control }
        return control + " " + String(localized: "It leaves \(roomName(other)).")
    }

    private func roomName(_ id: String) -> String {
        deps.missions.first { $0.id == id }?.name ?? String(localized: "the room")
    }

    private var terminalsInteractive: Bool {
        sheet == nil && !deps.workbench.showsPalette && deps.workbench.sharingSessionId == nil
    }

    private enum Sheet: Equatable {
        case signIn, recoveryKey, trust(FriendTrust), details(String), newRoom, history, launcher
    }

    private var sheet: Sheet? {
        let workbench = deps.workbench
        if deps.signInPresented {
            return .signIn
        }
        if deps.account.newRecoveryKey != nil {
            return .recoveryKey
        }
        if let trust = workbench.trusting {
            return .trust(trust)
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
        case let .trust(trust):
            SheetLayer(close: { workbench.trusting = nil }, content: { FriendTrustSheet(trust: trust) })
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
                    started: { workbench.launcher = nil }, close: { workbench.launcher = nil },
                )
                .padding(24)
            })
        }
    }

    private var notice: some View {
        ZStack {
            if deps.updateRequired {
                NoticePill(message: String(localized: "This version of Kodosi is too old for the server. Update Kodosi."),
                           identifier: "shell.update", action: deps.updater.configured ? { deps.updater.check() } : nil,
                           actionTitle: "Check for Updates", dismiss: { deps.updateRequired = false })
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            } else if let error = deps.errorMessage {
                NoticePill(message: error, identifier: "shell.error") { deps.errorMessage = nil }
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            } else if let device = deps.deviceNotice {
                NoticePill(message: device.message, identifier: "shell.devices", sign: .status(.hand), action: deps.openDeviceNotice,
                           actionTitle: device.actionTitle, dismiss: deps.dismissDeviceNotice)
                    .padding(.top, Metrics.frameInset + 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .padding(.leading, deps.workbench.shownSidebarWidth)
        .animation(theme.motion.spring, value: deps.errorMessage)
        .animation(theme.motion.spring, value: deps.updateRequired)
        .animation(theme.motion.spring, value: deps.deviceNotice)
    }
}

private extension DeviceNotice {
    var message: String {
        switch self {
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
        case .requests: "Approve…"
        case .approvalNeeded: "Open Settings"
        case .approvalWaiting: "Show code"
        }
    }
}
