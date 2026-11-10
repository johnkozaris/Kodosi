import KodosiBridge
import KodosiDesign
import SwiftUI

/// The part of a terminal's menu that changes the agent in front while it runs: only what the
/// agent can change without starting again.
struct AgentMenuSection: View {
    @Environment(AppModel.self) private var model
    let session: RuntimeSession
    let live: AgentLive

    private var agent: AgentKind {
        AgentKind(program: session.program)
    }

    private var settings: AgentSettings {
        session.programSettings ?? AgentSettings()
    }

    private var pending: AgentSettings {
        session.programPending ?? AgentSettings()
    }

    /// The models and efforts of the computer that runs the agent, when it is the person's own.
    private var launcher: LauncherState? {
        guard session.isOwner, let host = session.hostDeviceId else { return nil }
        return model.launcher(for: host)
    }

    private var models: AgentModels? {
        launcher?.models[agent.rawValue]
    }

    private var currentModel: String? {
        pending.model ?? settings.model
    }

    private var currentEffort: String? {
        pending.model != nil ? pending.effort : pending.effort ?? settings.effort
    }

    private var currentPermissions: AgentPermissions? {
        pending.permissions ?? settings.permissions
    }

    var body: some View {
        if shows {
            Section(agent.label) {
                if live.model, let models, !models.models.isEmpty {
                    Picker(selection: choice(currentModel ?? "") { AgentSettings(model: $0) }) {
                        ForEach(models.models) { option in
                            Text(verbatim: option.label).tag(option.id)
                        }
                    } label: {
                        Label("Model", systemImage: "sparkle")
                        Text(verbatim: shown(modelLabel, waits: pending.model != nil))
                    }
                    .pickerStyle(.menu)
                    .accessibilityIdentifier("terminal.menu.model")
                }
                if live.effort, let levels = models?.efforts(for: currentModel), !levels.isEmpty {
                    Picker(selection: choice(currentEffort ?? "") { AgentSettings(effort: $0) }) {
                        ForEach(levels) { level in
                            Text(verbatim: level.label).tag(level.id)
                        }
                    } label: {
                        Label("Effort", systemImage: "gauge.with.dots.needle.67percent")
                        Text(verbatim: shown(levels.first { $0.id == currentEffort }?.label ?? "", waits: pending.effort != nil))
                    }
                    .pickerStyle(.menu)
                    .accessibilityIdentifier("terminal.menu.effort")
                }
                if !live.permissions.isEmpty {
                    let options = AgentPermissions.allCases.filter { live.permissions.contains($0) || $0 == currentPermissions }
                    Picker(selection: choice(currentPermissions?.rawValue ?? "") { AgentSettings(permissions: AgentPermissions(rawValue: $0)) }) {
                        ForEach(options, id: \.self) { option in
                            Text(verbatim: option.label).tag(option.rawValue)
                        }
                    } label: {
                        Label("Permissions", systemImage: (currentPermissions ?? .ask).symbol)
                        Text(verbatim: shown(currentPermissions?.label ?? "", waits: pending.permissions != nil))
                    }
                    .pickerStyle(.menu)
                    .accessibilityIdentifier("terminal.menu.permissions")
                }
                if live.keeps, live.model || live.effort, models != nil {
                    Text("\(agent.label) also keeps the model and effort for its next sessions.")
                }
            }
            .task(id: session.program) { await launcher?.loadModels(agent.rawValue) }
        }
    }

    private var shows: Bool {
        session.canControl && ((launcher != nil && (live.model || live.effort)) || !live.permissions.isEmpty)
    }

    private var modelLabel: String {
        guard let currentModel else { return String(localized: "Default") }
        return models?.models.first { $0.id == currentModel }?.label ?? currentModel
    }

    private func shown(_ value: String, waits: Bool) -> String {
        waits ? String(localized: "\(value), after this turn") : value
    }

    private func choice(_ current: String, change: @escaping (String) -> AgentSettings) -> Binding<String> {
        Binding(get: { current }, set: { picked in
            if picked != current {
                model.terminals.changeAgent(change(picked), in: session)
            }
        })
    }
}
