import KodosiBridge
import KodosiDesign
import SwiftUI

extension View {
    func newTerminalSheet(_ request: Binding<NewTerminalRequest?>) -> some View {
        modifier(NewTerminalPresenter(request: request))
    }
}

private struct NewTerminalPresenter: ViewModifier {
    @Environment(AppModel.self) private var model
    @Environment(\.openRoute) private var openRoute
    @Binding var request: NewTerminalRequest?
    @State private var started: String?

    func body(content: Content) -> some View {
        content
            .sheet(item: $request, onDismiss: openStarted) { request in
                NewTerminalSheet(request: request, online: model.account.computers.map(\.deviceId)) { started = $0 }
            }
            .onChange(of: started.flatMap { model.terminals.session($0) } != nil) { openStarted() }
    }

    private func openStarted() {
        guard request == nil, let started, model.terminals.session(started) != nil else { return }
        self.started = nil
        openRoute(TerminalRoute(sessionId: started))
    }
}

struct NewTerminalSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var draft: NewTerminal
    @State private var folders: [FolderRoute] = []
    @State private var made = 0
    @FocusState private var typing: Bool
    let started: (String) -> Void

    init(request: NewTerminalRequest, online: [String], started: @escaping (String) -> Void) {
        _draft = State(initialValue: NewTerminal(request: request, online: online))
        self.started = started
    }

    private var computers: [MyDeviceEntry] {
        model.account.computers
    }

    private var computer: String? {
        let online = computers.map(\.deviceId)
        return draft.computer.flatMap { online.contains($0) ? $0 : nil } ?? online.first
    }

    private var launcher: LauncherState? {
        computer.map { model.launcher(for: $0) }
    }

    var body: some View {
        NavigationStack(path: $folders) {
            Group {
                if let launcher {
                    form(launcher)
                } else if !model.account.online, model.account.hasOtherDevices {
                    EmptyState(title: "Offline") {
                        IconTile(symbol: "wifi.slash", tint: TileTint.graphite, size: 56)
                    } actions: {
                        Button("Try again") { model.account.retryConnection() }
                            .buttonStyle(.kodosi(.secondary, size: .small))
                            .accessibilityIdentifier("terminals.new.unreachable.retry")
                    }
                    .accessibilityIdentifier("terminals.new.unreachable")
                } else {
                    EmptyState(title: "No computer is online", message: "Open Kodosi on one of your computers to start a terminal on it.") {
                        IconTile(symbol: "laptopcomputer", tint: TileTint.graphite, size: 56)
                    } actions: {
                        EmptyView()
                    }
                    .accessibilityIdentifier("terminals.new.offline")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(theme.colors.surface)
            .navigationTitle("New terminal")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .buttonStyle(.kodosi(.ghost, size: .small))
                        .fixedSize()
                        .accessibilityIdentifier("terminals.new.cancel")
                }
                .sharedBackgroundVisibility(.hidden)
            }
            .navigationDestination(for: FolderRoute.self) { route in
                folderPage(route)
            }
        }
        .onChange(of: computer) { folders.removeAll() }
        .presentationDetents([.large])
        .kodosiSheet()
        .interactiveDismissDisabled(draft.starting)
        .sensoryFeedback(.success, trigger: made)
    }

    private func name(of computer: String?) -> String {
        computers.first { $0.deviceId == computer }?.label ?? ""
    }

    private func folderPage(_ route: FolderRoute) -> some View {
        let launcher = model.launcher(for: route.computer)
        return FolderPage(
            launcher: launcher, computer: name(of: route.computer), path: route.path, current: draft.folder(in: launcher),
        ) { folder in
            folders.append(FolderRoute(computer: route.computer, path: folder))
        } pick: { folder, owner in
            draft.choose(folder: folder, in: owner)
            folders.removeAll()
        }
        .id(route.computer)
    }

    private func form(_ launcher: LauncherState) -> some View {
        let agent = draft.agent(in: launcher)
        let plan = agent.map { LaunchPlan(launcher: launcher, agent: $0.agent) }
        return ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                programs(launcher, chosen: agent)
                ListGroup {
                    computerRow(launcher)
                    folderRow(launcher)
                    if let agent, let plan {
                        if agent.accounts.count > 1 || agent.addsAccounts {
                            accountRow(agent, plan: plan, launcher: launcher)
                        }
                        if plan.listsModels {
                            modelRow(agent, plan: plan, launcher: launcher)
                        }
                        if !plan.efforts.isEmpty {
                            effortRow(plan, launcher: launcher)
                        }
                        if agent.permissions.count > 1 {
                            permissionsRow(agent, plan: plan, launcher: launcher)
                        }
                        if !agent.outputs.isEmpty {
                            outputRow(agent, plan: plan, launcher: launcher)
                        }
                    }
                }
                if let agent {
                    FirstPrompt(agent: agent, prompt: $draft.prompt, typing: $typing)
                        .terminalScope()
                        .transition(.opacity)
                }
                if let projectId = draft.projectId, let project = model.projects.project(projectId) {
                    HStack(spacing: 8) {
                        ProjectSigil(key: projectId, size: 18)
                        Text("In \(project.name)").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                    .padding(.leading, 4)
                    .accessibilityElement(children: .combine)
                }
                if let failure = draft.failure {
                    ErrorNote(message: failure)
                        .transition(AnyTransition.rise)
                        .accessibilityIdentifier("terminals.new.failure")
                }
            }
            .padding(20)
            .frame(maxWidth: 560, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) { startBar(launcher, agent: agent) }
        .animation(theme.motion.spring, value: agent?.agent)
        .animation(theme.motion.spring, value: draft.failure)
        .animation(theme.motion.spring, value: launcher.catalog)
        .task(id: launcher.computer) { await load(launcher) }
        .task(id: "\(launcher.computer ?? "")|\(agent?.agent ?? "")") {
            if let agent {
                await launcher.loadModels(agent.agent)
            }
        }
    }

    private func load(_ launcher: LauncherState, again: Bool = false) async {
        async let agents: Void = launcher.refresh(force: again)
        _ = try? await launcher.folders()
        await agents
    }

    private func programs(_ launcher: LauncherState, chosen: AgentInfo?) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            FlowLayout(spacing: 8) {
                ProgramChoice(id: "shell", kind: .shell, selected: chosen == nil) { launcher.choose(.tab(nil)) }
                ForEach(launcher.installed) { info in
                    ProgramChoice(id: info.agent, kind: info.kind, selected: chosen?.agent == info.agent) {
                        launcher.choose(.tab(info.agent))
                    }
                    .transition(AnyTransition.pop)
                }
                if launcher.catalog == nil, launcher.failure == nil {
                    ForEach(0 ..< 2, id: \.self) { _ in
                        SkeletonBlock(width: 112, height: 38, radius: 19)
                    }
                }
            }
            if launcher.catalog == nil, let failure = launcher.failure {
                ErrorNote(message: failure)
                Button("Try again") { Task { await load(launcher, again: true) } }
                    .buttonStyle(.kodosi(.secondary, size: .small))
                    .accessibilityIdentifier("terminals.new.retry")
            }
        }
    }

    private func computerRow(_ launcher: LauncherState) -> some View {
        let name = name(of: launcher.computer)
        return ChoiceRow(
            title: String(localized: "Computer"), value: name, values: computers.map(\.deviceId), selection: launcher.computer ?? "",
            label: { id in computers.first { $0.deviceId == id }?.label ?? id }, choose: { draft.computer = $0 },
            leading: { IconTile(symbol: DeviceGlyph.symbol(for: name), tint: TileTint.graphite, size: 28) },
        )
        .accessibilityIdentifier("terminals.new.computer")
    }

    private func folderRow(_ launcher: LauncherState) -> some View {
        let folder = draft.folder(in: launcher)
        let places = launcher.places
        return NavigationLink(value: FolderRoute(computer: launcher.computer ?? "", path: nil)) {
            SheetRow(title: String(localized: "Folder"), subtitle: folder.map { places?.shown($0) ?? $0 } ?? "~", monoSubtitle: true) {
                IconTile(symbol: "folder.fill", tint: TileTint.teal, size: 28)
            } trailing: {
                Disclosure()
            }
        }
        .buttonStyle(ListRowButtonStyle())
        .accessibilityIdentifier("terminals.new.folder")
    }

    private func accountRow(_ agent: AgentInfo, plan: LaunchPlan, launcher: LauncherState) -> some View {
        let account = plan.account
        let title = account?.title(program: agent.program) ?? ""
        return ChoiceRow(
            title: String(localized: "Account"), subtitle: account?.identity ?? String(localized: "Signs in when it starts"),
            value: title, values: agent.accounts.map(\.name), selection: account?.name ?? "",
            label: { name in agent.accounts.first { $0.name == name }?.title(program: agent.program) ?? name },
            choose: { name in choosePick(plan, launcher) { $0.account = name } },
            leading: {
                PersonAvatar(name: title, key: account?.signIn, size: 28,
                             color: agent.kind.tint(account: agent.accounts.firstIndex { $0.name == account?.name }))
            },
        )
        .accessibilityIdentifier("terminals.new.account")
    }

    private func modelRow(_ agent: AgentInfo, plan: LaunchPlan, launcher: LauncherState) -> some View {
        let ids: [String?] = [nil] + (plan.models?.models.map(\.id) ?? [plan.modelId].compactMap(\.self))
        let label = { (id: String?) in
            id.map { id in plan.models?.models.first { $0.id == id }?.label ?? id } ?? String(localized: "Default")
        }
        return ChoiceRow(
            title: String(localized: "Model"), value: label(plan.modelId),
            loading: plan.models == nil && launcher.loadingModels.contains(agent.agent),
            values: ids, selection: plan.modelId, label: label,
            choose: { model in choosePick(plan, launcher) { $0.model = model } },
            leading: { IconTile(symbol: "sparkle", tint: agent.kind.tint, size: 28) },
        )
        .accessibilityIdentifier("terminals.new.model")
    }

    private func effortRow(_ plan: LaunchPlan, launcher: LauncherState) -> some View {
        let level = plan.efforts.firstIndex { $0.id == plan.effortId }
        let label = { (id: String?) in id.map { id in plan.efforts.first { $0.id == id }?.label ?? id } ?? String(localized: "Auto") }
        return ChoiceRow(
            title: String(localized: "Effort"), value: label(plan.effortId), values: [nil] + plan.efforts.map(\.id),
            selection: plan.effortId, label: label, choose: { effort in choosePick(plan, launcher) { $0.effort = effort } },
            leading: { IconTile(symbol: "gauge.with.dots.needle.67percent", tint: TileTint.amber, size: 28) },
            accessory: {
                if let level {
                    EffortBars(level: level, count: plan.efforts.count, height: 12).transition(AnyTransition.pop)
                }
            },
        )
        .accessibilityIdentifier("terminals.new.effort")
    }

    private func permissionsRow(_ agent: AgentInfo, plan: LaunchPlan, launcher: LauncherState) -> some View {
        let permissions = plan.permissions
        return ChoiceRow(
            title: String(localized: "Permissions"), subtitle: permissions.detail(for: agent.kind), value: permissions.label,
            values: agent.permissions, selection: permissions, label: \.label,
            choose: { permissions in choosePick(plan, launcher) { $0.permissions = permissions } },
            leading: {
                IconTile(symbol: permissions.symbol, tint: permissions == .unrestricted ? TileTint.amber : agent.kind.tint, size: 28)
            },
        )
        .accessibilityIdentifier("terminals.new.permissions")
    }

    private func outputRow(_ agent: AgentInfo, plan: LaunchPlan, launcher: LauncherState) -> some View {
        ChoiceRow(
            title: String(localized: "Verbosity"), subtitle: plan.output.detail, value: plan.output.label,
            values: agent.outputs, selection: plan.output, label: \.label,
            choose: { output in choosePick(plan, launcher) { $0.output = output } },
            leading: { IconTile(symbol: "text.alignleft", tint: TileTint.graphite, size: 28) },
        )
        .accessibilityIdentifier("terminals.new.output")
    }

    private func choosePick(_ plan: LaunchPlan, _ launcher: LauncherState, _ change: (inout AgentPick) -> Void) {
        guard var pick = plan.shown else { return }
        change(&pick)
        launcher.choose(.pick(pick))
    }

    private func startBar(_ launcher: LauncherState, agent: AgentInfo?) -> some View {
        Button { start(launcher) } label: {
            Text(draft.starting ? "Starting…" : startTitle(agent))
                .lineLimit(2)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
                .shimmer(draft.starting)
        }
        .buttonStyle(.kodosi(.primary, size: .large))
        .disabled(draft.starting || !draft.ready(in: launcher))
        .accessibilityIdentifier("terminals.new.start")
        .padding(.horizontal, 20)
        .padding(.top, 8)
        .padding(.bottom, 12)
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
        .background(theme.colors.surface)
    }

    private func startTitle(_ agent: AgentInfo?) -> LocalizedStringKey {
        agent.map { "Start \($0.kind.label)" } ?? "Open a terminal"
    }

    private func start(_ launcher: LauncherState) {
        typing = false
        Task {
            guard let id = await draft.start(in: launcher, on: model.terminals) else { return }
            made += 1
            started(id)
            dismiss()
        }
    }
}
