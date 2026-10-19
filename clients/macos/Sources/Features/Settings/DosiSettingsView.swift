import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var instructions: DosiInstructions?
    @State private var draft = ""
    @State private var saving = false
    @State private var renewing = false
    @State private var editing = false
    @State private var message: String?

    private var setup: DosiSetup {
        deps.dosi.snapshot?.setup ?? DosiSetup()
    }

    private var installed: [DosiAgent] {
        deps.dosi.snapshot?.agents.filter(\.installed) ?? []
    }

    private var agent: String? {
        setup.agent ?? installed.first?.agent
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 14) {
                DosiFace(mood: deps.workbench.dosiOn ? deps.dosi.mood() : .off, size: 52)
                VStack(alignment: .leading, spacing: 4) {
                    Text(verbatim: "Dosi").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                    Text("Your agent for your agents").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer()
                if deps.workbench.dosiOn, installed.count > 1, let agent {
                    SegmentedPill(selection: Binding(get: { agent }, set: { choose(DosiSetup(agent: $0)) }),
                                  options: installed.map { SegmentOption(value: $0.agent, title: AgentKind(program: $0.agent).label) },
                                  compact: true, identifier: "settings.dosi.agent")
                }
            }
            ListGroup {
                Toggle(isOn: Binding(get: { deps.workbench.dosiOn }, set: turn)) {
                    HStack(spacing: 12) {
                        IconTile(symbol: "eyes", tint: TileTint.orange, size: 28)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Use Dosi")
                            Text("Off removes Dosi from this computer.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        }
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 52)
                .accessibilityIdentifier("settings.dosi.on")
            }
            if deps.workbench.dosiOn {
                details
            }
        }
        .animation(theme.motion.spring, value: deps.workbench.dosiOn)
        .confirmationDialog("Start a new chat?", isPresented: $renewing, titleVisibility: .visible) {
            Button("Start new chat", role: .destructive) { deps.renewDosi() }
            Button("Cancel", role: .cancel) {}
        } message: { Text("Dosi forgets this chat, here and on your other computers.") }
        .task { await deps.launcher.refresh() }
        .task(id: agent) {
            if let agent {
                await deps.launcher.loadModels(agent)
            }
        }
        .task(id: "\(deps.accountEpoch):\(deps.dosi.snapshot?.instructionsEdited ?? false)") { await loadInstructions() }
    }

    @ViewBuilder
    private var details: some View {
        if let agent {
            picks(agent)
        } else {
            ListGroup(footer: "Dosi runs on Claude Code or Codex. Install one, then come back.") {
                ForEach(deps.launcher.catalog?.agents.filter { ["claude", "codex"].contains($0.agent) } ?? []) { info in
                    ListRow(title: info.kind.label, subtitle: info.install, monoSubtitle: true) {
                        AgentMark(kind: info.kind, size: 28, activity: .asleep)
                    } trailing: {
                        Button("Copy") { deps.copyText(info.install) }.buttonStyle(.kodosi(.secondary, size: .small))
                    }
                }
            }
        }
        instructionsGroup
        ListGroup {
            ListRow(String(localized: "Dosi's folder"), subtitle: deps.dosi.snapshot?.folder, monoSubtitle: true,
                    symbol: "folder.fill", tint: TileTint.amber)
            {
                Button("Show in Finder") {
                    if let folder = deps.dosi.snapshot?.folder {
                        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: folder)
                    }
                }
                .buttonStyle(.kodosi(.secondary, size: .small)).accessibilityIdentifier("settings.dosi.folder")
            }
            ListRow(String(localized: "Chat"), subtitle: conversationLine, symbol: "bubble.left.and.bubble.right.fill",
                    tint: TileTint.orange)
            {
                Button("New chat") { renewing = true }
                    .buttonStyle(.kodosi(.secondary, size: .small)).disabled(deps.dosi.items.isEmpty)
                    .accessibilityIdentifier("settings.dosi.renew")
            }
            ListRow(String(localized: "Full access"), subtitle: String(localized: "Dosi uses every tool of its agent and does not ask first."),
                    symbol: "checkmark.shield.fill", tint: TileTint.green)
        }
        if let message {
            ErrorNote(message: message)
        }
    }

    private func turn(_ on: Bool) {
        if !on, deps.dosi.busy {
            deps.dosi.stop()
        }
        deps.workbench.dosiOn = on
    }

    private var conversationLine: String {
        let place = deps.dosi.snapshot?.shared == true
            ? String(localized: "on all your computers") : String(localized: "on this computer")
        guard let started = deps.dosi.snapshot?.conversation?.started else { return place }
        return String(localized: "Started \(started.formatted(date: .abbreviated, time: .shortened)), \(place)")
    }

    private func picks(_ agent: String) -> some View {
        let info = deps.launcher.agent(agent)
        let models = deps.launcher.models[agent]
        let efforts = models?.efforts(for: setup.model) ?? []
        return ListGroup(footer: "Dosi works in its own folder with the account that you choose here.") {
            ListRow(title: info?.kind.label ?? AgentKind(program: agent).label, subtitle: String(localized: "Account")) {
                AgentMark(kind: AgentKind(program: agent), size: 28, activity: .awake)
            } trailing: {
                MenuPicker(title: "Account", selection: Binding(get: { setup.account }, set: { pick(account: $0) }),
                           values: [nil] + (info?.accounts.map(\.name) ?? []).filter { $0 != "default" },
                           label: { name in
                               name.flatMap { name in info?.accounts.first { $0.name == name }?.title(program: info?.program ?? agent) }
                                   ?? String(localized: "Default")
                           }, identifier: "settings.dosi.account")
            }
            ListRow(String(localized: "Model"), symbol: "cpu", tint: TileTint.purple) {
                MenuPicker(title: "Model", selection: Binding(get: { setup.model }, set: { pick(model: $0) }),
                           values: [nil] + (models?.models.map(\.id) ?? []),
                           label: { id in
                               id.flatMap { id in models?.models.first { $0.id == id }?.label } ?? String(localized: "The agent's choice")
                           }, identifier: "settings.dosi.model")
            }
            if !efforts.isEmpty {
                ListRow(String(localized: "Effort"), symbol: "gauge.with.dots.needle.67percent", tint: TileTint.teal) {
                    MenuPicker(title: "Effort", selection: Binding(get: { setup.effort }, set: { pick(effort: $0) }),
                               values: [nil] + efforts.map(\.id),
                               label: { id in
                                   id.flatMap { id in efforts.first { $0.id == id }?.label } ?? String(localized: "The agent's choice")
                               }, identifier: "settings.dosi.effort")
                }
            }
        }
    }

    private var instructionsGroup: some View {
        let changed = instructions.map { draft != $0.text } ?? false
        return VStack(alignment: .leading, spacing: 8) {
            ListGroup(footer: editing ? "Dosi reads these at the start of each conversation, on all your computers." : nil) {
                ListRow(String(localized: "Instructions"),
                        subtitle: instructions?.edited == true ? String(localized: "Yours") : String(localized: "Standard"),
                        symbol: "text.alignleft", tint: TileTint.teal)
                {
                    HStack(spacing: 6) {
                        if editing, instructions?.edited == true {
                            Button("Use the standard ones") { save(nil) }
                                .buttonStyle(.kodosi(.ghost, size: .small)).disabled(saving)
                                .accessibilityIdentifier("settings.dosi.instructions.reset")
                        }
                        if editing, changed {
                            Button("Save") { save(draft) }
                                .buttonStyle(.kodosi(.primary, size: .small)).disabled(saving)
                                .accessibilityIdentifier("settings.dosi.instructions.save")
                                .transition(AnyTransition.pop)
                        } else {
                            Button(editing ? "Done" : "Edit") { editing.toggle() }
                                .buttonStyle(.kodosi(.secondary, size: .small)).disabled(instructions == nil)
                                .accessibilityIdentifier("settings.dosi.instructions.edit")
                        }
                    }
                }
            }
            if editing {
                TextEditor(text: $draft)
                    .font(.system(size: 12, design: .monospaced)).foregroundStyle(theme.colors.ink)
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: 240, maxHeight: 420)
                    .padding(10)
                    .well(Radius.lg)
                    .accessibilityIdentifier("settings.dosi.instructions")
                    .transition(AnyTransition.unfold)
            }
        }
        .animation(theme.motion.snappy, value: changed)
        .animation(theme.motion.spring, value: editing)
    }

    private func loadInstructions() async {
        guard instructions.map({ draft == $0.text }) ?? true else { return }
        do {
            let loaded = try await deps.dosi.instructions()
            instructions = loaded
            draft = loaded.text
        } catch { message = error.localizedDescription }
    }

    private func save(_ text: String?) {
        saving = true
        Task { @MainActor in
            defer { saving = false }
            do {
                let saved = try await deps.dosi.writeInstructions(text)
                instructions = saved
                draft = saved.text
                message = nil
            } catch { message = error.localizedDescription }
        }
    }

    private func pick(account: String?? = .none, model: String?? = .none, effort: String?? = .none) {
        var next = setup
        next.agent = agent
        if let account {
            next.account = account
        }
        if let model {
            next.model = model
            if let efforts = agent.flatMap({ deps.launcher.models[$0]?.efforts(for: model) }),
               !efforts.contains(where: { $0.id == next.effort })
            {
                next.effort = nil
            }
        }
        if let effort {
            next.effort = effort
        }
        choose(next)
    }

    private func choose(_ next: DosiSetup) {
        Task { @MainActor in
            do {
                try await deps.dosi.configure(next)
            } catch { message = error.localizedDescription }
        }
    }
}
