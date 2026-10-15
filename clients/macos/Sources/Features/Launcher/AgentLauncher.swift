import AppKit
import KodosiDesign
import SwiftUI

struct AgentLauncher: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var welcome = false
    @State private var folder: String?
    @State private var showsAll = false
    @State private var menu: LauncherMenu?
    @State private var adding = false
    @State private var saving = false
    @State private var addError: String?
    @State private var starting = false
    @State private var error: String?
    @State private var prompt = ""
    @State private var ghost = ""
    @State private var focus = 0
    @State private var seed: CGFloat = AgentLauncher.seeded ? 1 : 0
    @State private var opened = AgentLauncher.seeded

    private static var seeded = false
    private static let bandHeight: CGFloat = 42
    private static let width: CGFloat = 560

    private var launcher: LauncherState {
        deps.launcher
    }

    private var ink: AppTheme.Colors {
        theme.isDark ? theme.colors : AppTheme.dark.colors
    }

    private var choice: LaunchChoice {
        deps.settings.launchChoice
    }

    private var plan: LaunchPlan {
        LaunchPlan(choice: choice, launcher: launcher)
    }

    private var info: AgentInfo? {
        plan.info
    }

    private var directory: String? {
        folder ?? deps.settings.lastWorkingDir ?? deps.storageBootstrap.defaultWorkingDirectory?.path
    }

    private var takesWords: Bool {
        info?.installed ?? true
    }

    private var ready: Bool {
        info == nil ? choice.agent == nil : plan.line != nil
    }

    private var hint: String {
        info == nil ? String(localized: "Type a command") : String(localized: "Type a task")
    }

    private var tabItems: [AgentTabs.Item] {
        let agents = launcher.catalog?.agents ?? []
        let shown = agents.filter { $0.installed || showsAll || $0.agent == choice.agent }
        return [AgentTabs.Item(id: "shell", kind: .shell, installed: true)]
            + shown.map { AgentTabs.Item(id: $0.agent, kind: $0.kind, installed: $0.installed) }
    }

    var body: some View {
        VStack(alignment: welcome ? .center : .leading, spacing: welcome ? 22 : 14) {
            card
            actions.arrive(delay: welcome && !opened ? 1.2 : 0.08)
            if let error {
                ErrorNote(message: error).frame(maxWidth: 520).transition(.opacity)
            }
        }
        .animation(theme.motion.spring, value: choice)
        .animation(theme.motion.spring, value: launcher.catalog)
        .task { await launcher.refresh() }
        .task(id: choice.agent) {
            if let agent = choice.agent {
                await launcher.loadModels(agent)
            }
        }
        .task(id: "\(choice.agent ?? "")|\(prompt.isEmpty)|\(takesWords)") { await typeHints() }
        .onAppear(perform: grow)
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await launcher.refresh() }
        }
    }

    private func grow() {
        focus += 1
        guard !Self.seeded else { return }
        Self.seeded = true
        guard welcome, !theme.motion.reduced else {
            seed = 1
            opened = true
            return
        }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(450))
            withAnimation(.spring(duration: 0.95, bounce: 0.1)) { seed = 1 }
            try? await Task.sleep(for: .milliseconds(620))
            withAnimation(theme.motion.spring) { opened = true }
            focus += 1
        }
    }

    private var card: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        let cursor = CGRect(x: 16, y: Self.bandHeight + 18, width: 9, height: 18)
        return terminal
        .overlay(alignment: .topLeading) {
            if seed == 0 {
                CursorBlock(width: cursor.width, height: cursor.height)
                    .terminalScope()
                    .offset(x: cursor.minX, y: cursor.minY)
                    .transition(.opacity.animation(.easeOut(duration: 0.15)))
            }
        }
        .frame(width: Self.width)
        .clipShape(shape)
        .background {
            shape.fill(TerminalInk.body)
                .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.16), radius: 12, x: 3, y: 7)
        }
        .overlay { shape.strokeBorder(theme.colors.hairline.opacity(theme.isDark ? 0.7 : 0.9), lineWidth: 0.5).allowsHitTesting(false) }
        .mask { SeedMask(progress: seed, cursor: cursor, radius: Radius.lg) }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("launcher.preview")
    }

    private var terminal: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .bottom, spacing: 8) {
                if opened {
                    AgentTabs(
                        items: tabItems, selection: choice.agent ?? "shell",
                        hidden: (launcher.catalog?.agents ?? []).count { !$0.installed && $0.agent != choice.agent },
                        fresh: launcher.fresh, showsAll: $showsAll
                    ) { id in
                        update { $0.agent = id == "shell" ? nil : id }
                        closeMenu()
                    }
                    .transition(.opacity)
                    Spacer(minLength: 8)
                    HStack(spacing: 5) {
                        DeviceGlyph(label: String(localized: "This Mac"))
                        Text("This Mac")
                    }
                    .appTextStyle(.footnote).foregroundStyle(ink.inkMuted)
                    .padding(.bottom, 11)
                    .transition(.opacity)
                }
            }
            .padding(.horizontal, 10)
            .frame(maxWidth: .infinity, minHeight: Self.bandHeight, maxHeight: Self.bandHeight, alignment: .bottomLeading)
            .background(TerminalInk.band)
            VStack(alignment: .leading, spacing: 0) {
                if opened {
                    LauncherCommandLine(info: info, prompt: $prompt, ghost: ghost, hint: hint, focus: focus, submit: start) {
                        folderKey
                    } choices: {
                        if let info, info.accounts.count > 1 || info.addsAccounts {
                            accountKey(info).transition(AnyTransition.pop)
                        }
                        if let info, plan.listsModels || !plan.efforts.isEmpty {
                            modelKey(info).transition(AnyTransition.pop)
                        }
                    }
                }
            }
            .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 18)
            .frame(maxWidth: .infinity, minHeight: welcome ? 124 : 92, alignment: .topLeading)
            .background(TerminalInk.body)
            .contentShape(Rectangle())
            .onTapGesture { focus += 1 }
        }
        .terminalScope()
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
        let named = plan.model?.label ?? plan.modelArgument.map { String($0.split(separator: " ").last ?? "") }
        let level = plan.efforts.firstIndex { $0.argument == plan.effortArgument }
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
                    EffortMeter(levels: plan.efforts, current: plan.effortArgument) { argument in
                        update { $0.efforts[info.agent] = argument }
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
        focus += 1
    }

    private func pick(_ id: String) {
        guard let menu else { return }
        let agent = info?.agent ?? ""
        switch menu.pick(id) {
        case let .folder(path):
            folder = path
        case .anotherFolder:
            closeMenu()
            if let selected = pickWorkingDirectory(initialDirectory: directory) {
                folder = selected
            }
            return
        case .newAccount:
            withAnimation(theme.motion.snappy) { adding = true }
            return
        case let .account(command):
            update { $0.accounts[agent] = command }
        case let .model(argument):
            update { $0.models[agent] = argument }
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
                update { $0.accounts[info.agent] = account.command }
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
                        .bounce(on: prompt.isEmpty)
                }
                .shimmer(starting)
            }
            .buttonStyle(.kodosi(.primary))
            .keyboardShortcut(.defaultAction)
            .disabled(starting || !ready)
            .accessibilityIdentifier("launcher.start")
            IconButton(
                title: "Resume a conversation", symbol: "clock.arrow.circlepath", identifier: "launcher.resume", size: 32
            ) { deps.workbench.showsHistory = true }
        }
    }

    private var startTitle: LocalizedStringKey {
        guard let info else { return "Open a terminal" }
        return info.installed ? "Start \(info.kind.label)" : "Install \(info.kind.label)"
    }

    private func update(_ change: (inout LaunchChoice) -> Void) {
        change(&deps.settings.launchChoice)
        deps.settings.saveLaunchChoice()
        focus += 1
    }

    private func typeHints() async {
        ghost = ""
        guard prompt.isEmpty, takesWords else { return }
        guard !theme.motion.reduced else {
            ghost = hint
            return
        }
        try? await Task.sleep(for: .milliseconds(1200))
        for character in hint {
            guard !Task.isCancelled else { return }
            ghost.append(character)
            try? await Task.sleep(for: .milliseconds(character == " " ? 120 : Int.random(in: 38 ... 90)))
        }
    }

    private func start() {
        guard !starting, ready else { return }
        let words = prompt.trimmingCharacters(in: .whitespaces)
        let line = plan.line
        var command = line
        var first: String?
        if let info {
            if info.installed, !words.isEmpty {
                command = [line, info.promptFlag].compactMap(\.self).joined(separator: " ")
                first = words
            }
        } else if !words.isEmpty {
            command = words
        }
        starting = true
        error = nil
        let directory = directory
        Task { @MainActor in
            defer { starting = false }
            do {
                try await deps.createSession(name: generateSessionName(), directory: directory, command: command, prompt: first)
                prompt = ""
            } catch {
                withAnimation(theme.motion.fade) { self.error = error.localizedDescription }
            }
        }
    }
}
