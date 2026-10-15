import KodosiBridge
import KodosiDesign
import SwiftUI

struct AgentLauncher: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Binding var folder: String?
    var roomId: String?
    var centered = false
    var focus = 0
    var started: () -> Void = {}
    @State private var showsAll = false
    @State private var naming = false
    @State private var accountName = ""
    @State private var starting = false
    @State private var error: String?
    @State private var prompt = ""

    private static let width: CGFloat = 560

    private var launcher: LauncherState {
        deps.launcher
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
            terminal
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
        .alert(Text("New \(info?.kind.label ?? "") account"), isPresented: $naming) {
            TextField("Name", text: $accountName)
            Button("Add") { addAccount() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Kodosi makes a sign-in folder for it. \(info?.kind.label ?? "") asks you to sign in when it first starts.")
        }
    }

    private var terminal: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        return VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .bottom, spacing: 8) {
                AgentTabs(
                    items: tabItems, selection: choice.agent ?? "shell",
                    hidden: (launcher.catalog?.agents ?? []).count { !$0.installed && $0.agent != choice.agent },
                    showsAll: $showsAll
                ) { id in
                    update { $0.agent = id == "shell" ? nil : id }
                }
                Spacer(minLength: 8)
                place
            }
            .padding(.horizontal, 10)
            .frame(maxWidth: .infinity, minHeight: 42, maxHeight: 42, alignment: .bottomLeading)
            .background(TerminalInk.band)
            commandLine
                .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 18)
                .frame(maxWidth: .infinity, minHeight: centered ? 130 : 104, alignment: .topLeading)
                .background(TerminalInk.body)
        }
        .terminalScope()
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
        .appTextStyle(.footnote).foregroundStyle(TerminalInk.muted)
        .padding(.bottom, 11)
    }

    private var commandLine: some View {
        FlowLayout(spacing: 6) {
            folderKey
            Text(verbatim: "❯")
                .foregroundStyle(theme.colors.accent)
                .frame(height: 26)
            if let info {
                if info.installed {
                    agentKeys(info)
                } else {
                    Text(verbatim: info.install)
                        .foregroundStyle(TerminalInk.muted)
                        .textSelection(.enabled)
                        .frame(minHeight: 26)
                }
            }
            if takesWords {
                PromptField(
                    text: $prompt,
                    placeholder: info == nil ? String(localized: "command (optional)") : String(localized: "first prompt (optional)"),
                    label: info == nil ? String(localized: "Command") : String(localized: "First prompt"),
                    focus: focus + 1, submit: start
                )
                .layoutValue(key: FillsRow.self, value: 150)
                .accessibilityIdentifier("launcher.prompt")
            }
        }
        .font(.system(size: 13, weight: .medium, design: .monospaced))
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func agentKeys(_ info: AgentInfo) -> some View {
        let plan = plan
        if info.accounts.count > 1, let account = plan.account {
            Menu {
                ForEach(info.accounts) { item in
                    Toggle(isOn: Binding(get: { item.command == account.command }, set: { _ in
                        update { $0.accounts[info.agent] = item.command }
                    })) {
                        Text(item.title)
                        Text(item.detail)
                    }
                }
                if info.addsAccounts {
                    Divider()
                    Button("Add Account…") { askForAccount() }
                }
            } label: {
                LauncherKey(text: account.title, tint: info.kind.tint)
            }
            .launcherMenu(help: "Account", identifier: "launcher.account")
        }
        if let model = plan.modelArgument {
            Menu { modelPicker(plan) } label: { LauncherKey(text: plan.model?.label ?? model) }
                .launcherMenu(help: "Model", identifier: "launcher.model")
        }
        if let effort = plan.effortArgument {
            Menu { effortPicker(plan) } label: { LauncherKey(text: plan.efforts.first { $0.argument == effort }?.label ?? effort) }
                .launcherMenu(help: "Effort", identifier: "launcher.effort")
        }
        if info.addsAccounts || plan.models?.models.isEmpty != true || !plan.efforts.isEmpty {
            optionsMenu(info, plan)
        }
    }

    private func optionsMenu(_ info: AgentInfo, _ plan: LaunchPlan) -> some View {
        Menu {
            if plan.models == nil {
                Text(launcher.loadingModels.contains(info.agent) ? "Asking \(info.kind.label) for its models…" : "\(info.kind.label) did not list its models")
            } else {
                if plan.models?.models.isEmpty == false {
                    Menu("Model") { modelPicker(plan) }
                }
                if !plan.efforts.isEmpty {
                    Menu("Effort") { effortPicker(plan) }
                }
            }
            if info.addsAccounts {
                Divider()
                Button("Add Account…") { askForAccount() }
            }
        } label: {
            LauncherKey(symbol: "ellipsis")
        }
        .launcherMenu(help: "Model, effort and accounts", identifier: "launcher.options")
    }

    private func modelPicker(_ plan: LaunchPlan) -> some View {
        Picker(selection: binding(\.models, plan.info?.agent ?? "")) {
            Text("Default").tag(String?.none)
            ForEach(plan.models?.models ?? []) { model in
                Text(model.label).tag(Optional(model.argument))
            }
        } label: {
            EmptyView()
        }
        .pickerStyle(.inline)
    }

    private func effortPicker(_ plan: LaunchPlan) -> some View {
        Picker(selection: binding(\.efforts, plan.info?.agent ?? "")) {
            Text("Default").tag(String?.none)
            ForEach(plan.efforts) { effort in
                Text(effort.label).tag(Optional(effort.argument))
            }
        } label: {
            EmptyView()
        }
        .pickerStyle(.inline)
    }

    private var folderKey: some View {
        let recents = recentFolders(deps, chosen: folder)
        return Menu {
            Picker(selection: Binding(get: { directory }, set: { folder = $0 })) {
                ForEach(recents, id: \.self) { path in
                    Text(verbatim: tilde(path)).tag(Optional(path))
                }
            } label: {
                EmptyView()
            }
            .pickerStyle(.inline)
            Divider()
            Button("Choose Another Folder…") {
                if let selected = pickWorkingDirectory(initialDirectory: directory) {
                    folder = selected
                }
            }
        } label: {
            LauncherKey(text: directory.map(folderName) ?? "~", symbol: "folder.fill", tint: TerminalInk.path)
                .frame(maxWidth: 240, alignment: .leading)
        }
        .launcherMenu(help: "Folder", identifier: "launcher.folder")
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
            .buttonStyle(.kodosi(.primary))
            .keyboardShortcut(.defaultAction)
            .disabled(starting || !ready)
            .accessibilityIdentifier("launcher.start")
            Button("Resume…") {
                deps.workbench.launcher = nil
                deps.workbench.showsHistory = true
            }
            .buttonStyle(.kodosi(.secondary))
            .help("Continue a saved Claude Code or Copilot CLI conversation")
            .accessibilityIdentifier("launcher.resume")
        }
    }

    private var startTitle: LocalizedStringKey {
        guard let info else { return "Open a terminal" }
        return info.installed ? "Start \(info.kind.label)" : "Install \(info.kind.label)"
    }

    private func binding(_ path: WritableKeyPath<LaunchChoice, [String: String]>, _ agent: String) -> Binding<String?> {
        Binding(get: { choice[keyPath: path][agent] }, set: { value in update { $0[keyPath: path][agent] = value } })
    }

    private func update(_ change: (inout LaunchChoice) -> Void) {
        change(&deps.settings.launchChoice)
        deps.settings.saveLaunchChoice()
    }

    private func askForAccount() {
        accountName = ""
        naming = true
    }

    private func addAccount() {
        guard let info else { return }
        let name = accountName
        Task { @MainActor in
            do {
                let account = try await launcher.addAccount(info.agent, name: name)
                update { $0.accounts[info.agent] = account.command }
            } catch {
                withAnimation(theme.motion.fade) { self.error = error.localizedDescription }
            }
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
                try await deps.createSession(
                    name: generateSessionName(), directory: directory, roomId: roomId, command: command, prompt: first
                )
                prompt = ""
                started()
            } catch {
                withAnimation(theme.motion.fade) { self.error = error.localizedDescription }
            }
        }
    }
}
