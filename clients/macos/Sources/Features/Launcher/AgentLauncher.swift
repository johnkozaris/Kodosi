import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// The place to start work: pick an agent, and the command line that the new
/// terminal will run writes itself.
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

    private var launcher: LauncherState {
        deps.launcher
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

    private var dockItems: [AgentDock.Item] {
        let agents = launcher.catalog?.agents ?? []
        let shown = agents.filter { $0.installed || showsAll || $0.agent == choice.agent }
        return [AgentDock.Item(id: "shell", kind: .shell, installed: true)]
            + shown.map { AgentDock.Item(id: $0.agent, kind: $0.kind, installed: $0.installed) }
    }

    var body: some View {
        VStack(alignment: welcome ? .center : .leading, spacing: welcome ? 24 : 16) {
            AgentDock(
                items: dockItems, selection: choice.agent ?? "shell",
                hidden: (launcher.catalog?.agents ?? []).count { !$0.installed && $0.agent != choice.agent },
                showsAll: $showsAll
            ) { id in
                update { $0.agent = id == "shell" ? nil : id }
                showsEffort = false
            }
            .arrive()
            preview.arrive(delay: 0.05)
            actions.arrive(delay: 0.1)
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
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await launcher.refresh() }
        }
    }

    private var preview: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                ForEach(0 ..< 3, id: \.self) { _ in
                    Circle().fill(theme.colors.inkFaint.opacity(0.35)).frame(width: 7, height: 7)
                }
                Spacer(minLength: 12)
                folderToken
            }
            Spacer(minLength: 18)
            commandLine
            if let identity = info?.installed == true ? account?.identity : nil {
                Text(verbatim: "# \(identity)")
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(theme.colors.inkFaint)
                    .lineLimit(1).truncationMode(.middle)
                    .padding(.top, 10).padding(.leading, 2)
                    .transition(.opacity.combined(with: .offset(y: -4)))
            }
            Spacer(minLength: 18)
        }
        .padding(.horizontal, 18).padding(.vertical, 14)
        .frame(width: 540, alignment: .leading)
        .frame(minHeight: welcome ? 180 : 140)
        .fixedSize(horizontal: false, vertical: true)
        .raised(Radius.lg)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("launcher.preview")
    }

    private var commandLine: some View {
        FlowLayout(spacing: 6, lineSpacing: 6) {
            Image(systemName: "chevron.right")
                .font(.system(size: 13, weight: .heavy))
                .foregroundStyle(theme.colors.inkFaint)
                .frame(height: 26)
            if let info {
                if info.installed {
                    agentTokens(info)
                } else {
                    TypedText(text: info.install)
                        .foregroundStyle(theme.colors.inkMuted)
                        .frame(height: 26)
                }
            }
            PromptCursor(line: line ?? "")
                .padding(.leading, 2)
                .frame(height: 26)
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
                        .foregroundStyle(theme.colors.inkMuted)
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
            text: directory.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "~",
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
        .font(.system(size: 12, weight: .medium, design: .monospaced))
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
                }
                .shimmer(starting)
            }
            .buttonStyle(.kodosi(.primary, size: .large))
            .keyboardShortcut(.defaultAction)
            .disabled(starting || (info != nil && line == nil))
            .accessibilityIdentifier("launcher.start")
            Button { deps.workbench.showsHistory = true } label: {
                Label("Resume a conversation", systemImage: "clock.arrow.circlepath")
            }
            .buttonStyle(.kodosi(.ghost))
        }
    }

    private var startTitle: LocalizedStringKey {
        guard let info else { return "Open a terminal" }
        return info.installed ? "Start \(info.kind.label)" : "Install \(info.kind.label)"
    }

    private func update(_ change: (inout LaunchChoice) -> Void) {
        change(&deps.settings.launchChoice)
        deps.settings.saveLaunchChoice()
    }

    private func start() {
        guard !starting, info == nil || line != nil else { return }
        starting = true
        error = nil
        let line = line
        let directory = directory
        Task { @MainActor in
            defer { starting = false }
            do {
                try await deps.createSession(name: generateSessionName(), directory: directory, command: line)
            } catch {
                withAnimation(theme.motion.fade) { self.error = error.localizedDescription }
            }
        }
    }
}

private struct AccountChoices: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let info: AgentInfo
    let selected: String
    let choose: (String) -> Void
    @State private var adding = false
    @State private var name = ""
    @State private var saving = false
    @State private var error: String?
    @FocusState private var focused: Bool

    var body: some View {
        ChoicePanel(title: "Account") {
            ForEach(info.accounts) { account in
                ChoiceRow(
                    title: account.name,
                    detail: account.identity,
                    selected: account.command == selected,
                    identifier: "launcher.account.\(AccessibilityIdentifier.token(account.name))"
                ) { choose(account.command) }
            }
            if info.addsAccounts {
                if adding {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 8) {
                            TextField("Name, such as work", text: $name)
                                .textFieldStyle(WellTextFieldStyle()).focused($focused)
                                .onSubmit(add).accessibilityIdentifier("launcher.account.name")
                            Button("Add", action: add).buttonStyle(.kodosi(.primary, size: .small))
                                .disabled(saving || name.trimmingCharacters(in: .whitespaces).isEmpty)
                                .accessibilityIdentifier("launcher.account.add")
                        }
                        if let error {
                            ErrorNote(message: error)
                        }
                    }
                    .padding(.horizontal, 8).padding(.top, 4)
                    .onAppear { focused = true }
                } else {
                    ChoiceRow(title: String(localized: "Add an account"), symbol: "plus", identifier: "launcher.account.new") {
                        withAnimation(theme.motion.snappy) { adding = true }
                    }
                }
            }
        }
    }

    private func add() {
        let name = name.trimmingCharacters(in: .whitespaces)
        guard !saving, !name.isEmpty else { return }
        saving = true
        error = nil
        Task { @MainActor in
            defer { saving = false }
            do {
                let account = try await deps.launcher.addAccount(info.agent, name: name)
                choose(account.command)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

private struct ModelChoices: View {
    @Environment(\.theme) private var theme
    let models: [AgentModel]?
    let loading: Bool
    let selected: String?
    let choose: (String?) -> Void
    @State private var filter = ""

    private var shown: [AgentModel] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return models ?? [] }
        return (models ?? []).filter { $0.label.localizedCaseInsensitiveContains(query) || $0.id.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        ChoicePanel(title: "Model") {
            if (models?.count ?? 0) > 8 {
                TextField("Find a model", text: $filter)
                    .textFieldStyle(WellTextFieldStyle())
                    .padding(.horizontal, 4).padding(.bottom, 2)
                    .accessibilityIdentifier("launcher.model.filter")
            }
            ScrollView {
                VStack(spacing: 0) {
                    ChoiceRow(title: String(localized: "Default"), detail: String(localized: "The agent's own choice"),
                              selected: selected == nil, identifier: "launcher.model.default") { choose(nil) }
                    if models == nil {
                        Text(loading ? "Asking the agent" : "The agent did not list its models")
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .shimmer(loading).padding(10)
                    }
                    ForEach(shown) { model in
                        ChoiceRow(
                            title: model.label,
                            detail: model.label.caseInsensitiveCompare(model.id) == .orderedSame ? nil : model.id, monoDetail: true,
                            selected: model.argument == selected,
                            identifier: "launcher.model.\(AccessibilityIdentifier.token(model.id))"
                        ) { choose(model.argument) }
                    }
                }
            }
            .frame(maxHeight: 340)
            .fixedSize(horizontal: false, vertical: (models?.count ?? 0) <= 8)
        }
    }
}

private struct EffortChoices: View {
    let efforts: [AgentOption]
    let selected: String?
    let choose: (String?) -> Void

    var body: some View {
        ChoicePanel(title: "Effort") {
            ChoiceRow(title: String(localized: "Default"), detail: String(localized: "The model's own level"),
                      selected: selected == nil, identifier: "launcher.effort.default") { choose(nil) }
            ForEach(efforts) { effort in
                ChoiceRow(title: effort.label, selected: effort.argument == selected,
                          identifier: "launcher.effort.\(effort.id)") { choose(effort.argument) }
            }
        }
    }
}

private struct FolderChoices: View {
    let recents: [String]
    let selected: String?
    let pick: (String) -> Void
    let choose: () -> Void

    var body: some View {
        ChoicePanel(title: "Folder") {
            ForEach(recents, id: \.self) { path in
                ChoiceRow(
                    title: URL(fileURLWithPath: path).lastPathComponent, detail: tilde(path),
                    monoDetail: true, selected: path == selected,
                    identifier: "launcher.folder.\(AccessibilityIdentifier.token(path))"
                ) { pick(path) }
            }
            ChoiceRow(title: String(localized: "Choose a folder…"), symbol: "folder", identifier: "launcher.folder.choose", action: choose)
        }
    }
}

/// `path` with `~` for the home folder.
private func tilde(_ path: String) -> String {
    let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
    return path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}
