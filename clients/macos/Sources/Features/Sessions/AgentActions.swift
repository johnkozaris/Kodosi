import KodosiBridge
import KodosiDesign
import SwiftUI

struct AgentActions: View {
    enum Flyout {
        case model, permissions, output
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.closePanel) private var close
    let session: RuntimeSession
    let agent: AgentKind
    let live: AgentLive
    let stageId: String
    @Binding var flyout: Flyout?

    private var settings: AgentSettings {
        session.programSettings ?? AgentSettings()
    }

    private var pending: AgentSettings {
        session.programPending ?? AgentSettings()
    }

    private var launcher: LauncherState? {
        if session.kind == .local {
            return deps.launcher
        }
        guard session.isOwner, let host = session.hostDeviceId else { return nil }
        return deps.launcher(for: host)
    }

    private var models: AgentModels? {
        launcher?.models[agent.rawValue]
    }

    private var changesModel: Bool {
        (live.model || live.effort) && launcher != nil
    }

    private var model: String? {
        pending.model ?? settings.model
    }

    private var effort: String? {
        pending.model != nil ? pending.effort : pending.effort ?? settings.effort
    }

    private var permissions: AgentPermissions? {
        pending.permissions ?? settings.permissions
    }

    private var output: AgentOutput? {
        settings.output
    }

    var body: some View {
        if changesModel || !live.permissions.isEmpty || output != nil {
            PanelMenuHeader(verbatim: agent.label)
                .task { await launcher?.loadModels(agent.rawValue) }
            if changesModel {
                FlyoutRow(
                    title: Text("Model"), icon: "sparkle", value: modelValue, waits: pending.model != nil || pending.effort != nil,
                    agent: agent, open: flyoutBinding(.model), identifier: "\(stageId).agent.model",
                ) {
                    if let level = effortLevel {
                        EffortBars(level: level.index, count: level.count, height: 10)
                    }
                }
                .kodosiFlyout(isPresented: flyout == .model) { modelFlyout.chromeScope() }
            }
            if !live.permissions.isEmpty {
                FlyoutRow(
                    title: Text("Permissions"), icon: (permissions ?? .ask).symbol, value: permissions?.label ?? "",
                    waits: pending.permissions != nil, agent: agent, caution: permissions == .unrestricted,
                    open: flyoutBinding(.permissions), identifier: "\(stageId).agent.permissions",
                ) { EmptyView() }
                    .kodosiFlyout(isPresented: flyout == .permissions) { permissionsFlyout.chromeScope() }
            }
            if let output {
                FlyoutRow(
                    title: Text("Verbosity"), icon: output.symbol, value: output.label, waits: false, agent: agent,
                    open: flyoutBinding(.output), identifier: "\(stageId).agent.output",
                ) { EmptyView() }
                    .kodosiFlyout(isPresented: flyout == .output) { outputFlyout(output).chromeScope() }
            }
            PanelMenuDivider()
        }
    }

    private var modelValue: String {
        guard let model else { return String(localized: "Default") }
        return models?.models.first { $0.id == model }?.label ?? model
    }

    private var effortLevel: (index: Int, count: Int)? {
        guard let models, let effort else { return nil }
        let levels = models.efforts(for: model)
        return levels.firstIndex { $0.id == effort }.map { ($0, levels.count) }
    }

    private func flyoutBinding(_ kind: Flyout) -> Binding<Bool> {
        Binding(get: { flyout == kind }, set: { flyout = $0 ? kind : flyout == kind ? nil : flyout })
    }

    private func change(_ change: AgentSettings) {
        deps.changeAgent(change, in: session)
        flyout = nil
        close()
    }

    private var modelFlyout: some View {
        VStack(spacing: 0) {
            if live.model {
                PickerList(
                    rows: modelRows, identifier: "\(stageId).agent.models", width: 260,
                    pick: { change(AgentSettings(model: $0)) }, close: { flyout = nil },
                )
            }
            if live.effort, let levels = models.map({ $0.efforts(for: model) }), !levels.isEmpty {
                Rectangle().fill(theme.colors.hairline.opacity(0.6)).frame(height: 0.5)
                EffortMeter(levels: levels, current: effort, auto: false) { effort in
                    if let effort {
                        change(AgentSettings(effort: effort))
                    }
                }
            }
            note(keeps: live.keeps)
        }
        .frame(width: 260)
        .task { await launcher?.loadModels(agent.rawValue) }
    }

    private var modelRows: [PickerRow] {
        guard let models else {
            let loading = launcher?.loadingModels.contains(agent.rawValue) ?? false
            return [PickerRow(
                id: "menu.models", kind: .note,
                title: loading ? String(localized: "Asking \(agent.label) for its models") : String(localized: "\(agent.label) lists no models"),
            )]
        }
        return models.models.map { option in
            PickerRow(
                id: option.id, title: option.label,
                detail: option.label.caseInsensitiveCompare(option.id) == .orderedSame ? nil : option.id,
                current: option.id == model,
            )
        }
    }

    /// The modes that the agent takes only when it starts show too, so that a person sees why
    /// they can't choose them.
    private var permissionOptions: [AgentPermissions] {
        let known = launcher?.agent(agent.rawValue)?.permissions ?? []
        return AgentPermissions.allCases.filter { known.contains($0) || live.permissions.contains($0) || $0 == permissions }
    }

    private var permissionsFlyout: some View {
        VStack(spacing: 0) {
            ModeRow(
                title: String(localized: "Permissions"), identifier: "\(stageId).agent.permissions.modes",
                options: permissionOptions.map { option in
                    let enabled = live.permissions.contains(option)
                    return ModeRow.Option(
                        id: option.rawValue, label: option.label,
                        detail: enabled || option == permissions ? option.detail(for: agent)
                            : String(localized: "\(agent.label) takes this only when it starts."),
                        caution: option == .unrestricted, enabled: enabled,
                    )
                },
                current: permissions?.rawValue ?? "", pick: { id in
                    if let picked = AgentPermissions(rawValue: id), picked != permissions {
                        change(AgentSettings(permissions: picked))
                    }
                },
            )
            note(keeps: false)
        }
        .frame(width: 300)
    }

    /// No agent changes its output while it runs.
    private func outputFlyout(_ output: AgentOutput) -> some View {
        let known = launcher?.agent(agent.rawValue)?.outputs ?? []
        return ModeRow(
            title: String(localized: "Verbosity"), identifier: "\(stageId).agent.output.modes",
            options: AgentOutput.allCases.filter { known.contains($0) || $0 == output }.map { option in
                ModeRow.Option(
                    id: option.rawValue, label: option.label,
                    detail: option == output ? option.detail : String(localized: "\(agent.label) takes this only when it starts."),
                    caution: false, enabled: false,
                )
            },
            current: output.rawValue, pick: { _ in },
        )
        .frame(width: 300)
    }

    @ViewBuilder
    private func note(keeps: Bool) -> some View {
        let busy = session.working || session.needsUser
        if busy || keeps {
            Text(verbatim: [
                busy ? String(localized: "\(agent.label) changes when it finishes its turn.") : nil,
                keeps ? String(localized: "\(agent.label) also keeps the model and effort for its next sessions.") : nil,
            ].compactMap(\.self).joined(separator: " "))
                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14).padding(.bottom, 12)
        }
    }
}

private struct FlyoutRow<Accessory: View>: View {
    @Environment(\.theme) private var theme
    let title: Text
    let icon: String
    let value: String
    let waits: Bool
    let agent: AgentKind
    var caution = false
    @Binding var open: Bool
    let identifier: String
    @ViewBuilder let accessory: () -> Accessory
    @State private var hovered = false

    var body: some View {
        let lit = hovered || open
        Button { open.toggle() } label: {
            HStack(spacing: 9) {
                Image(systemName: icon).font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(caution ? theme.colors.caution : lit ? theme.colors.ink : theme.colors.inkMuted)
                    .frame(width: 18)
                title.font(.system(size: 12.5, weight: .medium)).foregroundStyle(theme.colors.ink).lineLimit(1)
                Spacer(minLength: 12)
                if waits {
                    Image(systemName: "clock").font(.system(size: 10, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                        .help(String(localized: "\(agent.label) changes when it finishes its turn."))
                        .transition(AnyTransition.pop)
                }
                Text(verbatim: value).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted).lineLimit(1).fixedSize()
                accessory()
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(lit ? theme.colors.inkMuted : theme.colors.inkFaint)
            }
            .padding(.horizontal, 8).frame(height: 30)
            .background {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                    .fill(lit ? theme.colors.raised.mix(with: theme.colors.ink, by: 0.06) : .clear)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { inside in
            hovered = inside
            if inside {
                open = true
            }
        }
        .animation(theme.motion.hover, value: lit)
        .animation(theme.motion.snappy, value: waits)
        .accessibilityLabel(title)
        .accessibilityValue(Text(verbatim: value))
        .accessibilityAddTraits(open ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }
}

extension View {
    func closesFlyout(_ flyout: Binding<AgentActions.Flyout?>) -> some View {
        onHover { inside in
            if inside {
                flyout.wrappedValue = nil
            }
        }
    }
}
