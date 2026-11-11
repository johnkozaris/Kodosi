import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalInputActions {
    let type: () -> Void
    let compose: () -> Void
    let hide: () -> Void
}

struct TerminalControls: View {
    @Environment(\.theme) private var theme
    @Bindable var controller: TerminalController
    let session: RuntimeSession
    let input: TerminalInput
    let compact: Bool
    var composing: FocusState<Bool>.Binding
    let actions: TerminalInputActions
    let menu: TerminalMenu?

    private var note: String? {
        controller.inputNote ?? controller.inputError
    }

    var body: some View {
        VStack(spacing: 8) {
            if input.showsAsk, session.needsUser {
                TerminalAsk(session: session, answers: controller.canType, press: controller.press)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            if let note {
                CautionNote(message: note, identifier: "terminal.inputNote", dismiss: controller.dismissInputNotes)
                    .transition(.opacity)
            }
            if input.showsKeys {
                TerminalKeyBar(enabled: controller.canType, compact: compact, press: controller.press, actions: actions, menu: menu)
                    .transition(.opacity)
            } else {
                TerminalComposer(
                    controller: controller, session: session, compact: compact, composing: composing, actions: actions,
                    focusesOnAppear: input == .composing,
                )
                .transition(.opacity)
            }
        }
        .padding(.horizontal, Self.margin(compact: compact))
        .padding(.top, input == .reading ? 8 : 4)
        .padding(.bottom, input == .reading && !compact ? 8 : 4)
        .animation(theme.motion.spring, value: session.needsUser)
        .animation(theme.motion.soft, value: note)
        .animation(theme.motion.soft, value: input)
    }

    static func margin(compact: Bool) -> CGFloat {
        compact ? 8 : 12
    }
}

private struct TerminalAsk: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let answers: Bool
    let press: (TerminalKey) -> Void

    var body: some View {
        let sign = session.sign(unseen: false) ?? .hand
        HStack(spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                StatusSign(form: sign, size: 13)
                ZStack(alignment: .leading) {
                    Text(session.programStatus?.caption ?? sign.label)
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(2)
                        .id(session.programStatus?.caption)
                        .transition(AnyTransition.roll)
                }
                .clipped()
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 0)
            if answers {
                HStack(spacing: 6) {
                    ForEach(TerminalKey.answers) { key in
                        TerminalKeyCap(key: key, highlighted: true) { press(key) }
                    }
                }
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, answers ? 6 : 12)
        .padding(.vertical, answers ? 6 : 8)
        .background(theme.colors.accentSoft, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("terminal.ask")
    }
}

private struct TerminalKeyBar: View {
    let enabled: Bool
    let compact: Bool
    let press: (TerminalKey) -> Void
    let actions: TerminalInputActions
    let menu: TerminalMenu?

    var body: some View {
        TerminalKeyRow(height: compact ? 40 : 42) {
            ForEach(TerminalKey.groups, id: \.self) { group in
                ForEach(group) { key in
                    TerminalKeyCap(key: key, fills: true) { press(key) }
                        .disabled(!enabled)
                        .layoutValue(key: StartsKeyGroup.self, value: key == group.first)
                }
            }
            TerminalActionCap(title: "Write a message", symbol: "square.and.pencil", fills: true, action: actions.compose)
                .accessibilityIdentifier("terminal.keys.compose")
                .layoutValue(key: StartsKeyGroup.self, value: true)
            if let menu {
                menu
            }
            TerminalActionCap(title: "Hide keyboard", symbol: "keyboard.chevron.compact.down", fills: true, action: actions.hide)
                .accessibilityIdentifier("terminal.keys.hide")
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Terminal keys"))
        .accessibilityIdentifier("terminal.keys")
    }
}

nonisolated struct TerminalKeyRow: Layout {
    static let gap: CGFloat = 3
    static let groupGap: CGFloat = 8
    static let widestKey: CGFloat = 64
    let height: CGFloat

    /// The frames of the keys of `groups`, each the count of keys in a group, across `width`.
    static func frames(groups: [Int], width: CGFloat, height: CGFloat) -> [CGRect] {
        let count = groups.reduce(0, +)
        guard count > 0 else { return [] }
        let gaps = CGFloat(count - groups.count) * gap
        let key = min(widestKey, max(0, ((width - gaps - CGFloat(groups.count - 1) * groupGap) / CGFloat(count)).rounded(.down)))
        let spread = groups.count > 1 ? (width - CGFloat(count) * key - gaps) / CGFloat(groups.count - 1) : 0
        var frames: [CGRect] = []
        var x: CGFloat = 0
        for (index, size) in groups.enumerated() {
            if index > 0 {
                x += spread - gap
            }
            for _ in 0 ..< size {
                frames.append(CGRect(x: x, y: 0, width: key, height: height))
                x += key + gap
            }
        }
        return frames
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        CGSize(width: proposal.width ?? CGFloat(subviews.count) * Self.widestKey, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        let groups = subviews.reduce(into: [Int]()) { groups, subview in
            if groups.isEmpty || subview[StartsKeyGroup.self] {
                groups.append(1)
            } else {
                groups[groups.count - 1] += 1
            }
        }
        for (subview, frame) in zip(subviews, Self.frames(groups: groups, width: bounds.width, height: bounds.height)) {
            subview.place(at: CGPoint(x: bounds.minX + frame.minX, y: bounds.minY), proposal: ProposedViewSize(frame.size))
        }
    }
}

private nonisolated struct StartsKeyGroup: LayoutValueKey {
    static let defaultValue = false
}

private nonisolated struct KeySlot: Shape {
    func path(in rect: CGRect) -> Path {
        Path(rect.insetBy(dx: -TerminalKeyRow.gap / 2, dy: min(0, (rect.height - 44) / 2)))
    }
}

struct TerminalCapFace: ViewModifier {
    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    let highlighted: Bool
    var fills = false

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        let lit = highlighted && isEnabled
        let face = sized(content.foregroundStyle(isEnabled ? lit ? theme.colors.onAccent : theme.colors.ink : theme.colors.inkFaint))
            .background {
                BezelBackground(shape: shape, fill: lit ? theme.colors.accent : theme.buttonFace(), bezel: lit ? .solid : .quiet)
            }
        if fills {
            face.contentShape(KeySlot())
        } else {
            face.touchTarget()
        }
    }

    @ViewBuilder
    private func sized(_ label: some View) -> some View {
        if fills {
            label
                .lineLimit(1)
                .minimumScaleFactor(0.75)
                .padding(.horizontal, 1)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            label
                .frame(minWidth: 42, minHeight: 36)
                .padding(.horizontal, 4)
        }
    }
}

private struct TerminalActionCap: View {
    let title: LocalizedStringKey
    let symbol: String
    var fills = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .modifier(TerminalCapFace(highlighted: false, fills: fills))
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityLabel(Text(title))
    }
}

private struct TerminalKeyCap: View {
    @Environment(\.theme) private var theme
    let key: TerminalKey
    var highlighted = false
    var fills = false
    let action: () -> Void

    var body: some View {
        let button = Button(action: action) {
            label.modifier(TerminalCapFace(highlighted: highlighted, fills: fills))
        }
        .buttonStyle(PressScaleStyle())
        .animation(theme.motion.spring, value: highlighted)
        .accessibilityLabel(Text(key.title))
        .accessibilityIdentifier("terminal.key.\(key.id)")
        if key == .interrupt {
            button.keyboardShortcut(".", modifiers: .command)
        } else {
            button
        }
    }

    @ViewBuilder
    private var label: some View {
        if let symbol = key.symbol {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
        } else if let cap = key.cap {
            Text(verbatim: cap)
                .font(.system(size: 13, weight: .semibold, design: .monospaced))
        }
    }
}

private struct TerminalComposer: View {
    @Environment(\.theme) private var theme
    @Bindable var controller: TerminalController
    let session: RuntimeSession
    let compact: Bool
    var composing: FocusState<Bool>.Binding
    let actions: TerminalInputActions
    let focusesOnAppear: Bool

    private var placeholder: String {
        session.agent.isAgent ? String(localized: "Message \(session.agent.label)") : String(localized: "Type a command")
    }

    private var canSend: Bool {
        controller.canType && TerminalMessage.text(from: controller.draft) != nil
    }

    var body: some View {
        let focused = composing.wrappedValue
        HStack(alignment: .bottom, spacing: 8) {
            leadingKey(focused: focused)
                .padding(.bottom, 4)
            HStack(alignment: .bottom, spacing: 6) {
                TextField(placeholder, text: $controller.draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .tint(theme.colors.accent)
                    .lineLimit(1 ... (compact ? 2 : 5))
                    .focused(composing)
                    .textInputAutocapitalization(session.agent.isAgent ? .sentences : .never)
                    .autocorrectionDisabled(!session.agent.isAgent)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .padding(.leading, 11)
                    .padding(.vertical, 7)
                    .accessibilityIdentifier("terminal.composer.field")
                SendKey(enabled: canSend, size: 32, action: send)
                    .keyboardShortcut(.return, modifiers: .command)
                    .accessibilityLabel(Text("Send"))
                    .accessibilityIdentifier("terminal.composer.send")
            }
            .composerField(focused: focused)
        }
        .animation(theme.motion.hover, value: focused)
        .animation(theme.motion.snappy, value: canSend)
        .task {
            if focusesOnAppear {
                composing.wrappedValue = true
            }
        }
    }

    @ViewBuilder
    private func leadingKey(focused: Bool) -> some View {
        if focused {
            TerminalActionCap(title: "Hide keyboard", symbol: "keyboard.chevron.compact.down", action: actions.hide)
                .accessibilityIdentifier("terminal.composer.hide")
        } else if controller.allowsKeyboard {
            TerminalActionCap(title: "Type in the terminal", symbol: "keyboard", action: actions.type)
                .accessibilityIdentifier("terminal.composer.type")
        }
    }

    private func send() {
        if controller.send() {
            composing.wrappedValue = false
        }
    }
}
