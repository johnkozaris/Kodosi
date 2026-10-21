import KodosiBridge
import KodosiDesign
import SwiftUI

struct AgentLauncher: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var roomId: String?
    var defaultFolder: String?
    var centered = false
    var focus = 0
    var started: () -> Void = {}
    @State private var showsAll = false
    @State private var menu: LauncherMenu?
    @State private var adding = false
    @State private var saving = false
    @State private var addError: String?
    @State private var starting = false
    @State private var error: String?
    @State private var focusRequest = 1

    private static let bandHeight: CGFloat = 42
    private static let width: CGFloat = 560

    private var launcher: LauncherState {
        deps.launcher
    }

    private var draft: LauncherDraft {
        deps.workbench.launcherDraft
    }

    private var folder: String? {
        draft.folder ?? defaultFolder
    }

    private var ink: AppTheme.Colors {
        theme.isDark ? theme.colors : AppTheme.dark.colors
    }

    private var choice: AgentChoice {
        launcher.choice
    }

    private var plan: LaunchPlan {
        LaunchPlan(launcher: launcher)
    }

    private var info: AgentInfo? {
        plan.info
    }

    private var directory: String? {
        folder ?? deps.settings.lastWorkingDir ?? deps.storageBootstrap.defaultWorkingDirectory?.path
    }

    private var ready: Bool {
        guard let info else { return choice.agent == nil }
        return !info.installed || plan.start != nil
    }

    private var hint: String {
        info == nil ? String(localized: "Type a command") : String(localized: "Type a task")
    }

    private var roomName: String? {
        roomId.flatMap { id in deps.missions.first { $0.id == id }?.name }
    }

    private var tabItems: [AgentTabs.Item] {
        let agents = launcher.catalog?.agents ?? []
        let shown = agents.filter { $0.installed || showsAll || $0.agent == choice.agent }
        return [AgentTabs.Item(id: "shell", kind: .shell, installed: true)]
            + shown.map { AgentTabs.Item(id: $0.agent, kind: $0.kind, installed: $0.installed) }
    }

    var body: some View {
        VStack(alignment: centered ? .center : .leading, spacing: centered ? 22 : 14) {
            card
            actions
            if let error {
                ErrorNote(message: error).frame(maxWidth: 520).transition(.opacity)
            }
        }
        .animation(theme.motion.spring, value: choice)
        .animation(theme.motion.spring, value: launcher.catalog)
        .task { await launcher.refresh() }
        .task(id: "\(choice.agent ?? "")|\(info?.installed == true)") {
            if let agent = choice.agent {
                await launcher.loadModels(agent)
            }
        }
    }

    private var card: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        return terminal
            .frame(width: Self.width)
            .clipShape(shape)
            .background {
                shape.fill(TerminalInk.body)
                    .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.16), radius: 12, x: 3, y: 7)
            }
            .overlay { shape.strokeBorder(theme.colors.hairline.opacity(theme.isDark ? 0.7 : 0.9), lineWidth: 0.5).allowsHitTesting(false) }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("launcher.preview")
    }

    private var terminal: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .bottom, spacing: 8) {
                AgentTabs(
                    items: tabItems, selection: choice.agent ?? "shell",
                    hidden: (launcher.catalog?.agents ?? []).count { !$0.installed && $0.agent != choice.agent },
                    showsAll: $showsAll
                ) { id in
                    choose(.tab(id == "shell" ? nil : id))
                    closeMenu()
                }
                Spacer(minLength: 8)
                place
            }
            .padding(.horizontal, 10)
            .frame(maxWidth: .infinity, minHeight: Self.bandHeight, maxHeight: Self.bandHeight, alignment: .bottomLeading)
            .background(TerminalInk.band)
            LauncherCommandLine(
                info: info, prompt: Bindable(deps.workbench).launcherDraft.prompt, hint: hint, focus: focus + focusRequest, submit: start
            ) {
                folderKey
            } choices: {
                if let info, info.accounts.count > 1 || info.addsAccounts {
                    accountKey(info).transition(AnyTransition.pop)
                }
                if let info, plan.listsModels || !plan.efforts.isEmpty {
                    modelKey(info).transition(AnyTransition.pop)
                }
            }
            .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 18)
            .frame(maxWidth: .infinity, minHeight: centered ? 124 : 92, alignment: .topLeading)
            .background(TerminalInk.body)
            .contentShape(Rectangle())
            .onTapGesture { focusRequest += 1 }
        }
        .terminalScope()
    }

    private var place: some View {
        HStack(spacing: 5) {
            DeviceGlyph(label: String(localized: "This Mac"))
            Text("This Mac")
            if let roomName {
                Text(verbatim: "·")
                RoomSigil(key: roomId ?? "", size: 13)
                Text(roomName).lineLimit(1)
            }
        }
        .appTextStyle(.footnote).foregroundStyle(ink.inkMuted)
        .padding(.bottom, 11)
    }

    private var folderKey: some View {
        PickerKey(
            title: directory.map(folderName) ?? "~", open: menu == .folder, identifier: "launcher.folder",
            label: String(localized: "Folder"), help: directory.map(tilde)
        ) {
            Image(systemName: "folder.fill").font(.system(size: 10, weight: .semibold)).foregroundStyle(TileTint.teal)
        } action: {
            toggle(.folder)
        }
        .kodosiPopover(isPresented: presented(.folder), alignment: .leading) {
            PickerList(
                rows: LauncherMenu.folder.rows(plan, recents: recentFolders(deps, chosen: folder), directory: directory, loading: false),
                identifier: "launcher.list.folder", width: 300, pick: pick, close: closeMenu
            )
        }
    }

    private func accountKey(_ info: AgentInfo) -> some View {
        let account = plan.account
        let title = account?.title(program: info.program) ?? ""
        return PickerKey(
            title: title, open: menu == .account, identifier: "launcher.account",
            label: String(localized: "Account"), help: account?.identity
        ) {
            AccountMark(initial: String(title.prefix(1)), key: account?.signIn ?? "", signedIn: account?.identity != nil, size: 16)
        } action: {
            toggle(.account)
        }
        .kodosiPopover(isPresented: presented(.account), alignment: .leading) {
            if adding {
                NewAccountForm(
                    agent: info.kind.label, folder: info.accounts.first?.folder ?? "~/.\(info.program)", error: addError, saving: saving
                ) { add($0, to: info) }
            } else {
                PickerList(
                    rows: LauncherMenu.account.rows(plan, recents: [], directory: directory, loading: false),
                    identifier: "launcher.list.account", width: 330, pick: pick, close: closeMenu
                )
            }
        }
    }

    private func modelKey(_ info: AgentInfo) -> some View {
        let named = plan.model?.label ?? plan.modelId
        let level = plan.efforts.firstIndex { $0.id == plan.effortId }
        return PickerKey(
            title: named ?? String(localized: "Model"), quiet: named == nil,
            loading: plan.models == nil && launcher.loadingModels.contains(info.agent),
            open: menu == .model, identifier: "launcher.model", label: String(localized: "Model and effort")
        ) {
            Image(systemName: "sparkle").font(.system(size: 9, weight: .bold)).foregroundStyle(info.kind.tint)
        } accessory: {
            if let level {
                EffortBars(level: level, count: plan.efforts.count, height: 10)
                    .transition(AnyTransition.pop)
            }
        } action: {
            toggle(.model)
        }
        .kodosiPopover(isPresented: presented(.model), alignment: .leading) {
            VStack(spacing: 0) {
                PickerList(
                    rows: LauncherMenu.model.rows(plan, recents: [], directory: directory, loading: launcher.loadingModels.contains(info.agent)),
                    identifier: "launcher.list.model", width: 290, pick: pick, close: closeMenu
                )
                if !plan.efforts.isEmpty {
                    Rectangle().fill(theme.colors.hairline.opacity(0.6)).frame(height: 0.5)
                    EffortMeter(levels: plan.efforts, current: plan.effortId) { effort in
                        choosePick { $0.effort = effort }
                        closeMenu()
                    }
                }
            }
            .frame(width: 290)
        }
    }

    private func presented(_ kind: LauncherMenu) -> Binding<Bool> {
        Binding {
            menu == kind
        } set: { shown in
            if !shown, menu == kind {
                closeMenu()
            }
        }
    }

    private func toggle(_ kind: LauncherMenu) {
        guard menu != kind else { return closeMenu() }
        withAnimation(theme.motion.snappy) {
            menu = kind
            adding = false
            addError = nil
        }
    }

    private func closeMenu() {
        guard menu != nil else { return }
        withAnimation(theme.motion.snappy) {
            menu = nil
            adding = false
            addError = nil
        }
        focusRequest += 1
    }

    private func pick(_ id: String) {
        guard let menu else { return }
        switch menu.pick(id) {
        case let .folder(path):
            deps.workbench.launcherDraft.folder = path
        case .anotherFolder:
            closeMenu()
            if let selected = pickWorkingDirectory(initialDirectory: directory) {
                deps.workbench.launcherDraft.folder = selected
            }
            return
        case .newAccount:
            withAnimation(theme.motion.snappy) { adding = true }
            return
        case let .account(name):
            choosePick { $0.account = name }
        case let .model(model):
            choosePick { $0.model = model }
        }
        closeMenu()
    }

    private func add(_ name: String, to info: AgentInfo) {
        guard !saving else { return }
        saving = true
        addError = nil
        Task { @MainActor in
            defer { saving = false }
            do {
                let account = try await launcher.addAccount(info.agent, name: name)
                choosePick { $0.account = account.name }
                closeMenu()
            } catch {
                withAnimation(theme.motion.fade) { addError = error.localizedDescription }
            }
        }
    }

    private var actions: some View {
        HStack(spacing: 10) {
            Button(action: start) {
                HStack(spacing: 8) {
                    Text(startTitle)
                    Image(systemName: "return").font(.system(size: 11, weight: .bold)).opacity(0.6)
                        .bounce(on: draft.prompt.isEmpty)
                }
                .shimmer(starting)
            }
            .buttonStyle(.kodosi(.primary))
            .keyboardShortcut(.defaultAction)
            .disabled(starting || !ready)
            .accessibilityIdentifier("launcher.start")
            IconButton(
                title: "Resume a conversation", symbol: "clock.arrow.circlepath", identifier: "launcher.resume", size: 32
            ) {
                deps.workbench.launcher = nil
                deps.workbench.showsHistory = true
            }
        }
    }

    private var startTitle: LocalizedStringKey {
        guard let info else { return "Open a terminal" }
        return info.installed ? "Start \(info.kind.label)" : "Install \(info.kind.label)"
    }

    private func choose(_ change: AgentChoiceChange) {
        launcher.choose(change)
        focusRequest += 1
    }

    private func choosePick(_ change: (inout AgentPick) -> Void) {
        guard var pick = plan.shown else { return }
        change(&pick)
        choose(.pick(pick))
    }

    private func start() {
        guard !starting, ready else { return }
        let words = draft.prompt.trimmingCharacters(in: .whitespaces)
        let installed = info?.installed == true
        let agent = installed ? plan.start : nil
        let command: String? = if let info {
            info.installed ? nil : info.install
        } else {
            words.isEmpty ? nil : words
        }
        let first = installed && !words.isEmpty ? words : nil
        starting = true
        error = nil
        let directory = directory
        Task { @MainActor in
            defer { starting = false }
            do {
                try await deps.createSession(
                    name: generateSessionName(), directory: directory, roomId: roomId, command: command, agent: agent, prompt: first
                )
                deps.workbench.launcherDraft = LauncherDraft()
                started()
            } catch {
                withAnimation(theme.motion.fade) { self.error = error.localizedDescription }
                await launcher.refresh(force: true)
            }
        }
    }
}
