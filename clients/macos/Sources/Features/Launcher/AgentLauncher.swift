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
    @State private var showsEffort = false
    @State private var moreHovered = false
    @State private var starting = false
    @State private var error: String?
    @State private var prompt = ""
    @State private var ghost = ""
    @State private var hint = 0
    @State private var focus = 0
    @State private var seed: CGFloat = AgentLauncher.seeded ? 1 : 0
    @State private var opened = AgentLauncher.seeded

    /// Whether the terminal grew from its cursor in this run of the app.
    private static var seeded = false
    private static let bandHeight: CGFloat = 42

    private static let promptHints = [
        String(localized: "fix the failing test"),
        String(localized: "explain how this folder works"),
        String(localized: "review my last commit"),
        String(localized: "add a dark mode"),
        String(localized: "write tests for the parser"),
    ]
    private static let commandHints = ["git status", "npm run dev", "make test", "ls -la"]

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

    private var info: AgentInfo? {
        launcher.agent(choice.agent)
    }

    private var models: AgentModels? {
        choice.agent.flatMap { launcher.models[$0] }
    }

    private var account: AgentAccount? {
        guard let info else { return nil }
        return info.accounts.first { $0.command == choice.accounts[info.agent] } ?? info.accounts.first
    }

    private var modelArgument: String? {
        guard let info, let saved = choice.models[info.agent] else { return nil }
        guard let models else { return saved }
        return models.models.contains { $0.argument == saved } ? saved : nil
    }

    private var model: AgentModel? {
        models?.models.first { $0.argument == modelArgument }
    }

    private var efforts: [AgentOption] {
        let all = models?.efforts ?? []
        guard let model, !model.efforts.isEmpty else { return all }
        return all.filter { model.efforts.contains($0.id) }
    }

    private var effortArgument: String? {
        guard let info, let saved = choice.efforts[info.agent] else { return nil }
        guard models != nil else { return saved }
        return efforts.contains { $0.argument == saved } ? saved : nil
    }

    private var directory: String? {
        folder ?? deps.settings.lastWorkingDir ?? deps.storageBootstrap.defaultWorkingDirectory?.path
    }

    private var line: String? {
        guard let info else { return nil }
        guard info.installed else { return info.install }
        guard let account else { return nil }
        return ([account.command] + [modelArgument, effortArgument].compactMap(\.self)).joined(separator: " ")
    }

    /// Whether the person can type after the command: not while the line installs an agent.
    private var takesWords: Bool {
        info?.installed ?? true
    }

    /// Whether Start has a line to run: not while the chosen agent is still unknown.
    private var ready: Bool {
        info == nil ? choice.agent == nil : line != nil
    }

    private var suggestion: String {
        let hints = info == nil ? Self.commandHints : Self.promptHints
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
                        showsEffort = false
                    }
                    .transition(.opacity)
                    Spacer(minLength: 8)
                    HStack(spacing: 5) {
                        DeviceGlyph(label: String(localized: "This Mac")).font(.system(size: 11, weight: .medium))
                        Text("This Mac")
                    }
                    .appTextStyle(.caption).foregroundStyle(ink.inkMuted)
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
                    if let identity = info?.installed == true ? account?.identity : nil {
                        Text(verbatim: "# \(identity)")
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(ink.inkFaint)
                            .lineLimit(1).truncationMode(.middle)
                            .transition(.opacity.combined(with: .offset(y: -4)))
                    }
                }
            }
            .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 18)
            .frame(maxWidth: .infinity, minHeight: welcome ? 150 : 104, alignment: .topLeading)
            .background(TerminalInk.body)
            .contentShape(Rectangle())
            .onTapGesture { focus += 1 }
        }
        .overlay(alignment: .topLeading) {
            if seed == 0 {
                CursorBlock(width: cursor.width, height: cursor.height)
                    .offset(x: cursor.minX, y: cursor.minY)
                    .transition(.opacity.animation(.easeOut(duration: 0.15)))
            }
        }
        .terminalScope()
        .frame(width: 560)
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
        FlowLayout(spacing: 6, lineSpacing: 6) {
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
        .font(.system(size: 14, weight: .medium, design: .monospaced))
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(theme.motion.spring, value: line)
    }

    @ViewBuilder
    private func agentTokens(_ info: AgentInfo) -> some View {
        if let account {
            let named = account.command == account.name
            CommandToken(
                text: named ? account.name : info.program, badge: named ? nil : account.name, tint: info.kind.tint,
                identifier: "launcher.account", help: "Account"
            ) { close in
                AccountChoices(info: info, selected: account.command) { command in
                    update { $0.accounts[info.agent] = command }
                    close()
                }
            }
        }
        if models == nil || !(models?.models.isEmpty ?? true) {
            CommandToken(
                text: modelArgument ?? String(localized: "model"), slot: modelArgument == nil,
                loading: models == nil && launcher.loadingModels.contains(info.agent),
                identifier: "launcher.model", help: "Model"
            ) { close in
                ModelChoices(models: models?.models, loading: launcher.loadingModels.contains(info.agent), selected: modelArgument) { argument in
                    update { $0.models[info.agent] = argument }
                    close()
                }
            }
        }
        if !efforts.isEmpty {
            if effortArgument != nil || showsEffort {
                CommandToken(
                    text: effortArgument ?? String(localized: "effort"), slot: effortArgument == nil,
                    identifier: "launcher.effort", help: "Effort"
                ) { close in
                    EffortChoices(efforts: efforts, selected: effortArgument) { argument in
                        update { $0.efforts[info.agent] = argument }
                        if argument == nil {
                            showsEffort = false
                        }
                        close()
                    }
                }
                .transition(AnyTransition.pop)
            } else {
                Button {
                    withAnimation(theme.motion.spring) { showsEffort = true }
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 11, weight: .heavy))
                        .foregroundStyle(ink.inkMuted)
                        .frame(width: 30, height: 26)
                        .background { ChipFace(lit: moreHovered) }
                        .offset(y: moreHovered ? -0.5 : 0)
                        .contentShape(Rectangle())
                }
                .buttonStyle(PressScaleStyle(scale: 0.95))
                .onHover { moreHovered = $0 }
                .animation(theme.motion.hover, value: moreHovered)
                .help("More choices")
                .accessibilityIdentifier("launcher.moreChoices")
            }
        }
    }

    private var folderToken: some View {
        CommandToken(
            text: directory.map(tilde) ?? "~", symbol: "folder.fill", tint: hex(0x5DAFA6),
            identifier: "launcher.folder", help: "Folder"
        ) { close in
            FolderChoices(recents: recentFolders, selected: directory) { path in
                folder = path
                close()
            } choose: {
                close()
                if let selected = pickWorkingDirectory(initialDirectory: directory) {
                    folder = selected
                }
            }
        }
        .frame(maxWidth: 240, alignment: .leading)
    }

    private var recentFolders: [String] {
        var seen = Set<String>()
        let local: [String?] = deps.sessions.filter { $0.kind == .local }.map(\.localDirectory)
        let candidates: [String?] = [folder, deps.settings.lastWorkingDir] + local + [deps.storageBootstrap.defaultWorkingDirectory?.path]
        return Array(candidates.compactMap(\.self).filter { seen.insert($0).inserted }.prefix(6))
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
            .buttonStyle(.kodosi(.primary, size: .large))
            .keyboardShortcut(.defaultAction)
            .disabled(starting || !ready)
            .accessibilityIdentifier("launcher.start")
            IconButton(
                title: "Resume a conversation", symbol: "clock.arrow.circlepath", identifier: "launcher.resume", size: 36
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

    /// Types the suggestions into the empty line one after another, at a
    /// person's uneven pace, and takes each away again.
    private func typeHints() async {
        ghost = ""
        guard prompt.isEmpty, takesWords else { return }
        guard !theme.motion.reduced else {
            ghost = suggestion
            return
        }
        try? await Task.sleep(for: .milliseconds(1200))
        while !Task.isCancelled {
            for character in suggestion {
                ghost.append(character)
                try? await Task.sleep(for: .milliseconds(character == " " ? 120 : Int.random(in: 38 ... 90)))
                guard !Task.isCancelled else { return }
            }
            try? await Task.sleep(for: .milliseconds(2600))
            while !ghost.isEmpty, !Task.isCancelled {
                ghost.removeLast()
                try? await Task.sleep(for: .milliseconds(14))
            }
            try? await Task.sleep(for: .milliseconds(450))
            guard !Task.isCancelled else { return }
            hint += 1
        }
    }

    private func start() {
        guard !starting, ready else { return }
        let words = prompt.trimmingCharacters(in: .whitespaces)
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
