import KodosiBridge
import KodosiDesign
import SwiftUI

struct AgentDefaults: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let info: AgentInfo
    @State private var menu: LauncherMenu?

    private var launcher: LauncherState {
        deps.launcher
    }

    private var plan: LaunchPlan {
        LaunchPlan(launcher: launcher, agent: info.agent)
    }

    var body: some View {
        let plan = plan
        ListGroup(footer: "New terminals start with these. What you choose in the launcher changes them too.") {
            if info.accounts.count > 1 {
                account(plan)
            }
            if plan.listsModels || !plan.efforts.isEmpty {
                model(plan)
            }
            if info.permissions.count > 1 {
                let permissions = plan.permissions
                ModeSetting(
                    title: String(localized: "Permissions"), detail: permissions.detail(for: info.kind), symbol: permissions.symbol,
                    tint: permissions == .unrestricted ? TileTint.amber : info.kind.tint,
                    options: info.permissions.map { SegmentOption(value: $0, title: $0.label) }, current: permissions,
                    identifier: "settings.agents.permissions",
                ) { permissions in
                    choosePick { $0.permissions = permissions }
                }
            }
            if !info.outputs.isEmpty {
                let output = plan.output
                ModeSetting(
                    title: String(localized: "Verbosity"), detail: output.detail, symbol: output.symbol, tint: TileTint.graphite,
                    options: info.outputs.map { SegmentOption(value: $0, title: $0.label) }, current: output,
                    identifier: "settings.agents.output",
                ) { output in
                    choosePick { $0.output = output }
                }
            }
        }
        .task(id: info.agent) { await launcher.loadModels(info.agent) }
    }

    private func account(_ plan: LaunchPlan) -> some View {
        let account = plan.account
        let title = account?.title(program: info.program) ?? ""
        return ListRow(title: String(localized: "Account"), subtitle: account?.identity ?? String(localized: "Signs in when it starts")) {
            AccountMark(
                initial: String(title.prefix(1)), key: account?.signIn ?? "", signedIn: account?.identity != nil, size: 28,
                tint: info.kind.tint(account: info.accounts.firstIndex { $0.name == account?.name }),
            )
        } trailing: {
            ChoiceButton(title: title, open: presented(.account), identifier: "settings.agents.account", label: String(localized: "Account")) {
                PickerList(
                    rows: LauncherMenu.accounts(info, current: account?.name ?? "").filter { $0.kind != .action },
                    identifier: "settings.agents.list.account", width: 330, pick: pick, close: closeMenu,
                )
            }
        }
    }

    private func model(_ plan: LaunchPlan) -> some View {
        let loading = launcher.loadingModels.contains(info.agent)
        let title = plan.model?.label ?? plan.modelId ?? String(localized: "Default")
        let level = plan.efforts.firstIndex { $0.id == plan.effortId }
        return ListRow(String(localized: "Model"), symbol: "sparkle", tint: info.kind.tint) {
            ChoiceButton(
                title: title, value: [title, level.map { plan.efforts[$0].label }].compactMap(\.self).joined(separator: ", "),
                loading: plan.models == nil && loading, bars: level.map { (level: $0, count: plan.efforts.count) },
                open: presented(.model), identifier: "settings.agents.model", label: String(localized: "Model and effort"),
            ) {
                VStack(spacing: 0) {
                    PickerList(
                        rows: LauncherMenu.model.rows(plan, recents: [], directory: nil, loading: loading),
                        identifier: "settings.agents.list.model", width: 290, pick: pick, close: closeMenu,
                    )
                    if !plan.efforts.isEmpty {
                        Rectangle().fill(theme.colors.hairline.opacity(0.6)).frame(height: 0.5)
                        EffortMeter(levels: plan.efforts, current: plan.effortId, identifier: "settings.agents.effort") { effort in
                            choosePick { $0.effort = effort }
                            closeMenu()
                        }
                    }
                }
                .frame(width: 290)
            }
        }
    }

    private func presented(_ kind: LauncherMenu) -> Binding<Bool> {
        Binding {
            menu == kind
        } set: { shown in
            if shown {
                menu = kind
            } else if menu == kind {
                menu = nil
            }
        }
    }

    private func closeMenu() {
        guard menu != nil else { return }
        withAnimation(theme.motion.snappy) { menu = nil }
    }

    private func pick(_ id: String) {
        guard let menu else { return }
        switch menu.pick(id) {
        case let .account(name):
            choosePick { $0.account = name }
        case let .model(model):
            choosePick { $0.model = model }
        default:
            break
        }
        closeMenu()
    }

    private func choosePick(_ change: (inout AgentPick) -> Void) {
        guard var pick = plan.shown else { return }
        change(&pick)
        launcher.choose(.pick(pick))
    }
}

private struct ChoiceButton<Content: View>: View {
    @Environment(\.theme) private var theme
    let title: String
    var value: String?
    var loading = false
    var bars: (level: Int, count: Int)?
    @Binding var open: Bool
    let identifier: String
    let label: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        Button { open.toggle() } label: {
            HStack(spacing: 8) {
                Text(verbatim: title).lineLimit(1).shimmer(loading)
                if let bars {
                    EffortBars(level: bars.level, count: bars.count, height: 10).transition(AnyTransition.pop)
                }
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .buttonStyle(.kodosi(.secondary))
        .animation(theme.motion.spring, value: title)
        .accessibilityLabel(Text(verbatim: label))
        .accessibilityValue(Text(verbatim: value ?? title))
        .accessibilityIdentifier(identifier)
        .kodosiPopover(isPresented: $open, alignment: .trailing, content: content)
    }
}

private struct ModeSetting<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    let title: String
    let detail: String
    let symbol: String
    let tint: Color
    let options: [SegmentOption<Value>]
    let current: Value
    let identifier: String
    let pick: (Value) -> Void

    var body: some View {
        HStack(spacing: 12) {
            IconTile(symbol: symbol, tint: tint, size: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: title).appTextStyle(.body).foregroundStyle(theme.colors.ink)
                Text(verbatim: detail).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
                    .contentTransition(.interpolate)
            }
            Spacer(minLength: 12)
            SegmentedPill(selection: Binding(get: { current }, set: { picked in
                if picked != current {
                    pick(picked)
                }
            }), options: options, compact: true, identifier: identifier)
        }
        .padding(.horizontal, 14).padding(.vertical, 10).frame(minHeight: 52)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(verbatim: title))
    }
}
