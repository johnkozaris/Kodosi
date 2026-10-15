import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// The place to start work: a small terminal that shows the one about to
/// open. Pick an agent, and its command line writes itself; type the first
/// prompt after it, or a command for a plain terminal.
struct AgentLauncher: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var welcome = false
    @State private var folder: String?
    @State private var showsAll = false
    @State private var showsMore = false
    @State private var menu: LauncherMenu?
    @State private var keys: [LauncherMenu: CGFloat] = [:]
    @State private var adding = false
    @State private var saving = false
    @State private var addError: String?
    @State private var starting = false
    @State private var error: String?
    @State private var prompt = ""
    @State private var ghost = ""
    @State private var hint = Int.random(in: 0 ..< 20)
    @State private var focus = 0
    @State private var seed: CGFloat = AgentLauncher.seeded ? 1 : 0
    @State private var opened = AgentLauncher.seeded

    /// Whether the terminal grew from its cursor in this run of the app.
    private static var seeded = false
    private static let bandHeight: CGFloat = 42
    private static let width: CGFloat = 560
    private static let inset: CGFloat = 16
    private static let bodySpace = "launcher.body"

    private var launcher: LauncherState {
        deps.launcher
    }

    /// The colors inside the terminal, which is dark in both looks.
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

    /// Whether the person can type after the command: not while the line installs an agent.
    private var takesWords: Bool {
        info?.installed ?? true
    }

    /// Whether Start has a line to run: not while the chosen agent is still unknown.
    private var ready: Bool {
        info == nil ? choice.agent == nil : plan.line != nil
    }

    private var suggestion: String {
        let hints = info == nil ? LauncherMenu.commandHints : LauncherMenu.promptHints
        return hints[hint % hints.count]
    }

    private var tabItems: [AgentTabs.Item] {
        let agents = launcher.catalog?.agents ?? []
        let shown = agents.filter { $0.installed || showsAll || $0.agent == choice.agent }
        return [AgentTabs.Item(id: "shell", kind: .shell, installed: true)]
            + shown.map { AgentTabs.Item(id: $0.agent, kind: $0.kind, installed: $0.installed) }
    }

    var body: some View {
        VStack(alignment: welcome ? .center : .leading, spacing: welcome ? 22 : 14) {
            terminal
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

    /// The first time in a run of the app, the home's terminal grows from its
    /// copper cursor, as in the film; after that it is simply there.
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

    /// The terminal about to open, as its tile will look: a band with a tab for
    /// each kind of terminal, then the prompt with the folder, the command and
    /// the cursor.
    private var terminal: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        let cursor = CGRect(x: 16, y: Self.bandHeight + 18, width: 9, height: 18)
        return VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .bottom, spacing: 8) {
                if opened {
                    AgentTabs(
                        items: tabItems, selection: choice.agent ?? "shell",
                        hidden: (launcher.catalog?.agents ?? []).count { !$0.installed && $0.agent != choice.agent },
                        fresh: launcher.fresh, showsAll: $showsAll
                    ) { id in
                        update { $0.agent = id == "shell" ? nil : id }
                        showsMore = false
                        menu = nil
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
            VStack(alignment: .leading, spacing: 8) {
                if opened {
                    commandLine
                    if let menu {
                        menuView(menu)
                    } else if let identity = info?.installed == true ? plan.account?.identity : nil {
                        Text(verbatim: "# \(identity)")
                            .appTextStyle(.mono)
                            .foregroundStyle(ink.inkFaint)
                            .lineLimit(1).truncationMode(.middle)
                            .transition(.opacity.combined(with: .offset(y: -4)))
                    }
                }
            }
            .coordinateSpace(.named(Self.bodySpace))
            .padding(.horizontal, Self.inset).padding(.top, 14).padding(.bottom, 18)
            .frame(maxWidth: .infinity, minHeight: welcome ? 150 : 104, alignment: .topLeading)
            .background(TerminalInk.body)
            .contentShape(Rectangle())
            .onTapGesture {
                if menu == nil {
                    focus += 1
                } else {
                    closeMenu()
                }
            }
        }
        .overlay(alignment: .topLeading) {
            if seed == 0 {
                CursorBlock(width: cursor.width, height: cursor.height)
                    .offset(x: cursor.minX, y: cursor.minY)
                    .transition(.opacity.animation(.easeOut(duration: 0.15)))
            }
        }
        .terminalScope()
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

    private var commandLine: some View {
        FlowLayout(spacing: 6) {
            folderToken
            Text(verbatim: "❯")
                .foregroundStyle(ink.accent)
                .frame(height: 26)
            if let info {
                if info.installed {
                    agentTokens(info)
                } else {
                    TypedText(text: info.install)
                        .foregroundStyle(ink.inkMuted)
                        .frame(height: 26)
                    PromptCursor(line: info.install)
                        .frame(height: 26)
                }
            }
            if takesWords {
                PromptField(
                    text: $prompt, ghost: ghost, suggestion: suggestion,
                    label: info == nil ? String(localized: "Command") : String(localized: "First prompt"),
                    focus: focus, submit: start
                )
                .layoutValue(key: FillsRow.self, value: 150)
                .accessibilityIdentifier("launcher.prompt")
            }
        }
        .font(.system(size: 13, weight: .medium, design: .monospaced))
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(theme.motion.spring, value: plan.line)
    }

    @ViewBuilder
    private func agentTokens(_ info: AgentInfo) -> some View {
        let plan = plan
        if let account = plan.account {
            let named = account.command == account.name
            CommandToken(
                text: named ? account.name : info.program, badge: named ? nil : account.name, tint: info.kind.tint,
                open: menu == .account, identifier: "launcher.account", help: "Account"
            ) { toggle(.account) }
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.bodySpace)).minX } action: { keys[.account] = $0 }
        }
        if plan.modelArgument != nil || (showsMore && plan.listsModels) {
            CommandToken(
                text: plan.modelArgument ?? String(localized: "model"), slot: plan.modelArgument == nil,
                loading: plan.models == nil && launcher.loadingModels.contains(info.agent),
                open: menu == .model, identifier: "launcher.model", help: "Model"
            ) { toggle(.model) }
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.bodySpace)).minX } action: { keys[.model] = $0 }
                .transition(AnyTransition.pop)
        }
        if plan.effortArgument != nil || (showsMore && !plan.efforts.isEmpty) {
            CommandToken(
                text: plan.effortArgument ?? String(localized: "effort"), slot: plan.effortArgument == nil,
                open: menu == .effort, identifier: "launcher.effort", help: "Effort"
            ) { toggle(.effort) }
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.bodySpace)).minX } action: { keys[.effort] = $0 }
                .transition(AnyTransition.pop)
        }
        if !showsMore, (plan.modelArgument == nil && plan.listsModels) || (plan.effortArgument == nil && !plan.efforts.isEmpty) {
            MoreKey {
                withAnimation(theme.motion.spring) { showsMore = true }
            }
        }
    }

    private var folderToken: some View {
        CommandToken(
            text: directory.map(folderName) ?? "~", symbol: "folder.fill", tint: hex(0x5DAFA6),
            open: menu == .folder, identifier: "launcher.folder", help: "Folder"
        ) { toggle(.folder) }
            .frame(maxWidth: 240, alignment: .leading)
            .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.bodySpace)).minX } action: { keys[.folder] = $0 }
    }

    /// The list that a key opened, in the terminal under the line, starting under the key.
    private func menuView(_ menu: LauncherMenu) -> some View {
        let room = Self.width - Self.inset * 2
        let x = min(keys[menu] ?? 0, room - 400)
        return Group {
            if adding, menu == .account, let info {
                NewAccountLine(
                    folder: info.accounts.first?.folder ?? "~/.\(info.program)", error: addError, saving: saving
                ) { add($0, to: info) } cancel: {
                    withAnimation(theme.motion.snappy) {
                        adding = false
                        addError = nil
                    }
                }
            } else {
                TerminalMenu(
                    lines: menu.lines(plan, recents: recentFolders(deps, chosen: folder), directory: directory,
                                      loading: info.map { launcher.loadingModels.contains($0.agent) } ?? false),
                    room: room - x, identifier: "launcher.list.\(menu.rawValue)", pick: pick
                ) {
                    // The list also goes when the line of a new account takes its place.
                    if !adding {
                        closeMenu()
                    }
                }
            }
        }
        .padding(.leading, max(0, x))
        .transition(.opacity.combined(with: .offset(y: -6)))
    }

    private func toggle(_ kind: LauncherMenu) {
        guard menu != kind else { return closeMenu() }
        withAnimation(theme.motion.spring) {
            menu = kind
            adding = false
            addError = nil
        }
    }

    private func closeMenu() {
        withAnimation(theme.motion.spring) {
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
        case let .effort(argument):
            update { $0.efforts[agent] = argument }
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
                        .symbolEffect(.bounce, value: prompt.isEmpty)
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

    /// Types a suggestion into the empty line once, at a person's uneven pace,
    /// and leaves it there with the cursor.
    private func typeHints() async {
        ghost = ""
        guard prompt.isEmpty, takesWords else { return }
        guard !theme.motion.reduced else {
            ghost = suggestion
            return
        }
        try? await Task.sleep(for: .milliseconds(1200))
        for character in suggestion {
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
