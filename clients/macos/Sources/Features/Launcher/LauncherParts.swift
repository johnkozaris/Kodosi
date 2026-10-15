import KodosiDesign
import SwiftUI

struct TypedText: View {
    @Environment(\.theme) private var theme
    let text: String
    @State private var shown = ""
    @State private var typing: Task<Void, Never>?

    var body: some View {
        Text(verbatim: shown)
            .onChange(of: text, initial: true) { _, target in type(target) }
            .onDisappear { typing?.cancel() }
            .accessibilityLabel(Text(verbatim: text))
    }

    private func type(_ target: String) {
        typing?.cancel()
        guard !theme.motion.reduced else {
            shown = target
            return
        }
        typing = Task { @MainActor in
            let kept = shown.commonPrefix(with: target).count
            while shown.count > kept {
                shown.removeLast()
                try? await Task.sleep(for: .milliseconds(8))
                guard !Task.isCancelled else { return }
            }
            let rest = target.dropFirst(kept)
            let pace = max(7, min(22, 420 / max(rest.count, 1)))
            for character in rest {
                shown.append(character)
                try? await Task.sleep(for: .milliseconds(pace))
                guard !Task.isCancelled else { return }
            }
        }
    }
}

enum TerminalInk {
    static let body = hex(0x0D0B09)
    static let band = hex(0x1A1512)
    static let path = hex(0x5DAFA6)
}

struct LauncherCommandLine: View {
    @Environment(\.theme) private var theme
    let folder: String
    let info: AgentInfo?
    let plan: LaunchPlan
    @Binding var prompt: String
    let focus: Int
    let submit: () -> Void

    var body: some View {
        FlowLayout(spacing: 6) {
            TypedText(text: folder)
                .foregroundStyle(TerminalInk.path)
                .frame(height: 26)
            Text(verbatim: "❯")
                .foregroundStyle(theme.colors.accent)
                .frame(height: 26)
            if let info {
                if info.installed {
                    HStack(spacing: 0) {
                        TypedText(text: plan.account.map { $0.command == $0.name ? $0.name : info.program } ?? info.program)
                            .foregroundStyle(info.kind.tint.mix(with: .white, by: 0.35))
                        TypedText(text: [plan.modelArgument, plan.effortArgument].compactMap(\.self).map { " \($0)" }.joined())
                            .foregroundStyle(theme.colors.inkMuted)
                    }
                    .lineLimit(1)
                    .frame(height: 26)
                } else {
                    TypedText(text: info.install)
                        .foregroundStyle(theme.colors.inkMuted)
                        .frame(height: 26)
                    PromptCursor(line: info.install)
                        .frame(height: 26)
                }
            }
            if info?.installed ?? true {
                let placeholder = info == nil ? String(localized: "command (optional)") : String(localized: "first prompt (optional)")
                PromptField(
                    text: $prompt,
                    placeholder: placeholder,
                    label: info == nil ? String(localized: "Command") : String(localized: "First prompt"),
                    focus: focus, submit: submit
                )
                .layoutValue(key: FillsRow.self, value: max(150, PromptTextView.width(placeholder: placeholder)))
                .accessibilityIdentifier("launcher.prompt")
            }
        }
        .font(.system(size: 13, weight: .medium, design: .monospaced))
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(theme.motion.spring, value: plan.line)
    }
}

struct PromptCursor: View {
    @Environment(\.theme) private var theme
    let line: String
    @State private var lit = true

    var body: some View {
        CursorBlock(width: 9, height: 18, blinks: false)
            .opacity(lit ? 1 : 0)
            .task(id: "\(line)|\(theme.motion.reduced)") {
                lit = true
                guard !theme.motion.reduced else { return }
                try? await Task.sleep(for: .milliseconds(1100))
                while !Task.isCancelled {
                    withAnimation(.easeInOut(duration: 0.08)) { lit.toggle() }
                    try? await Task.sleep(for: .milliseconds(lit ? 560 : 440))
                }
            }
    }
}

struct AgentTabs: View {
    struct Item: Identifiable, Equatable {
        let id: String
        let kind: AgentKind
        let installed: Bool

        var title: String {
            kind == .shell ? String(localized: "Terminal") : kind.label
        }
    }

    @Environment(\.theme) private var theme
    @Namespace private var tab
    let items: [Item]
    let selection: String
    let hidden: Int
    @Binding var showsAll: Bool
    let select: (String) -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(items) { item in
                mark(item).transition(AnyTransition.pop)
            }
            if hidden > 0 {
                Button {
                    withAnimation(theme.motion.spring) { showsAll.toggle() }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .bold))
                        .rotationEffect(.degrees(showsAll ? 45 : 0))
                        .foregroundStyle(theme.colors.inkMuted)
                        .frame(width: 30, height: 30)
                        .hoverRow(radius: Radius.sm)
                        .padding(.bottom, 2)
                }
                .buttonStyle(PressScaleStyle())
                .help(showsAll ? String(localized: "Hide agents that are not on this Mac")
                    : String(localized: "Show \(hidden) more agents"))
                .accessibilityLabel(Text("More agents"))
                .accessibilityIdentifier("launcher.more")
            }
        }
        .animation(theme.motion.spring, value: items.map(\.id))
    }

    private func mark(_ item: Item) -> some View {
        let selected = item.id == selection
        return Button {
            withAnimation(theme.motion.snappy) { select(item.id) }
        } label: {
            HStack(spacing: 7) {
                AgentMark(kind: item.kind, size: 18, activity: item.installed ? .awake : .asleep)
                    .keyframeAnimator(initialValue: 1.0, trigger: selected) { mark, scale in
                        mark.scaleEffect(selected && !theme.motion.reduced ? scale : 1)
                    } keyframes: { _ in
                        SpringKeyframe(0.8, duration: 0.09, spring: .snappy)
                        SpringKeyframe(1.0, duration: 0.45, spring: .bouncy)
                    }
                if selected {
                    Text(item.title).appTextStyle(.subhead).fixedSize()
                        .foregroundStyle(item.installed ? theme.colors.ink : theme.colors.inkMuted)
                        .transition(.opacity.combined(with: .offset(x: -6)))
                }
            }
            .padding(.horizontal, selected ? 12 : 8)
            .frame(height: selected ? 34 : 30)
            .background {
                if selected {
                    TabShape(radius: 9, flare: 8)
                        .fill(TerminalInk.body)
                        .matchedGeometryEffect(id: "tab", in: tab)
                }
            }
            .hoverRow(radius: Radius.sm, selected: selected)
            .padding(.bottom, selected ? 0 : 2)
        }
        .buttonStyle(PressScaleStyle(scale: 0.96))
        .help(item.installed ? item.title : String(localized: "\(item.title) is not on this Mac"))
        .accessibilityLabel(Text(item.title))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("launcher.agent.\(item.id)")
    }
}

struct TabShape: Shape {
    let radius: CGFloat
    let flare: CGFloat

    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX - flare, y: rect.maxY))
        path.addQuadCurve(to: CGPoint(x: rect.minX, y: rect.maxY - flare), control: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(to: CGPoint(x: rect.minX + radius, y: rect.minY), control: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(to: CGPoint(x: rect.maxX, y: rect.minY + radius), control: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - flare))
        path.addQuadCurve(to: CGPoint(x: rect.maxX + flare, y: rect.maxY), control: CGPoint(x: rect.maxX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}
