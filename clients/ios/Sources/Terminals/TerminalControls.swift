import KodosiBridge
import KodosiDesign
import SwiftUI

enum TerminalInput {
    case reading, typing, composing

    var showsKeys: Bool {
        self == .typing
    }

    var showsAsk: Bool {
        self == .reading
    }

    func showsHeader(compact: Bool) -> Bool {
        !compact || self == .reading
    }
}

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
                TerminalInputNote(message: note, dismiss: controller.dismissInputNotes)
                    .transition(.opacity)
            }
            if input.showsKeys {
                TerminalKeyBar(enabled: controller.canType, answering: session.needsUser, press: controller.press, actions: actions, menu: menu)
                    .transition(.opacity)
            } else {
                TerminalComposer(
                    controller: controller, session: session, compact: compact, composing: composing, actions: actions,
                    focusesOnAppear: input == .composing,
                )
                .transition(.opacity)
            }
        }
        .padding(.horizontal, compact ? 8 : 12)
        .padding(.top, input == .reading ? 8 : 4)
        .padding(.bottom, input == .reading && !compact ? 8 : 4)
        .animation(theme.motion.spring, value: session.needsUser)
        .animation(theme.motion.soft, value: note)
        .animation(theme.motion.soft, value: input)
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
                    ForEach(TerminalKey.allCases.filter(TerminalKey.answers.contains)) { key in
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

private struct TerminalInputNote: View {
    @Environment(\.theme) private var theme
    let message: String
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(theme.colors.caution)
                .frame(width: 6, height: 6)
                .accessibilityHidden(true)
            Text(message)
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.ink)
                .lineLimit(2)
            Spacer(minLength: 4)
            Button("Dismiss", action: dismiss)
                .buttonStyle(.kodosi(.ghost, size: .small))
                .accessibilityIdentifier("terminal.inputNote.dismiss")
        }
        .padding(.leading, 12)
        .padding(.trailing, 4)
        .padding(.vertical, 4)
        .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("terminal.inputNote")
    }
}

private struct TerminalKeyBar: View {
    let enabled: Bool
    let answering: Bool
    let press: (TerminalKey) -> Void
    let actions: TerminalInputActions
    let menu: TerminalMenu?

    var body: some View {
        HStack(spacing: 6) {
            TerminalKeyRow(enabled: enabled, answering: answering, press: press)
            TerminalActionCap(title: "Write a message", symbol: "square.and.pencil", action: actions.compose)
                .accessibilityIdentifier("terminal.keys.compose")
            if let menu {
                menu
                    .labelStyle(.iconOnly)
                    .font(.system(size: 14, weight: .semibold))
                    .modifier(TerminalCapFace(highlighted: false))
            }
            TerminalActionCap(title: "Hide keyboard", symbol: "keyboard.chevron.compact.down", action: actions.hide)
                .accessibilityIdentifier("terminal.keys.hide")
        }
    }
}

struct TerminalKeyRow: View {
    let enabled: Bool
    let answering: Bool
    let press: (TerminalKey) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(TerminalKey.allCases) { key in
                    TerminalKeyCap(key: key, highlighted: answering && TerminalKey.answers.contains(key)) {
                        press(key)
                    }
                }
            }
            .padding(2)
        }
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.45)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Terminal keys"))
        .accessibilityIdentifier("terminal.keys")
    }
}

private struct TerminalCapFace: ViewModifier {
    @Environment(\.theme) private var theme
    let highlighted: Bool

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        content
            .foregroundStyle(highlighted ? theme.colors.onAccent : theme.colors.ink)
            .frame(minWidth: 42, minHeight: 36)
            .padding(.horizontal, 4)
            .background {
                BezelBackground(
                    shape: shape, fill: highlighted ? theme.colors.accent : theme.colors.lifted,
                    bezel: highlighted ? .solid : .quiet,
                )
            }
            .contentShape(shape)
    }
}

private struct TerminalActionCap: View {
    let title: LocalizedStringKey
    let symbol: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .modifier(TerminalCapFace(highlighted: false))
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityLabel(Text(title))
    }
}

private struct TerminalKeyCap: View {
    @Environment(\.theme) private var theme
    let key: TerminalKey
    let highlighted: Bool
    let action: () -> Void

    var body: some View {
        let button = Button(action: action) {
            label.modifier(TerminalCapFace(highlighted: highlighted))
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
                .font(.system(size: 14, weight: .semibold, design: .monospaced))
        }
    }
}

struct TerminalComposer: View {
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
        let shape = RoundedRectangle(cornerRadius: 22, style: .continuous)
        let focused = composing.wrappedValue
        HStack(alignment: .bottom, spacing: 8) {
            leadingKey(focused: focused)
                .padding(.bottom, 4)
            HStack(alignment: .bottom, spacing: 6) {
                TextField(placeholder, text: $controller.draft, axis: .vertical)
                    .lineLimit(1 ... (compact ? 2 : 5))
                    .focused(composing)
                    .textInputAutocapitalization(session.agent.isAgent ? .sentences : .never)
                    .autocorrectionDisabled(!session.agent.isAgent)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .padding(.leading, 16)
                    .padding(.vertical, 11)
                    .accessibilityIdentifier("terminal.composer.field")
                Button(action: send) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(canSend ? theme.colors.onAccent : theme.colors.inkFaint)
                        .frame(width: 34, height: 34)
                        .background {
                            if canSend {
                                BezelBackground(shape: Circle(), fill: theme.colors.accent)
                            } else {
                                Circle().fill(theme.colors.ink.opacity(0.08))
                            }
                        }
                        .contentShape(Circle())
                }
                .buttonStyle(PressScaleStyle())
                .disabled(!canSend)
                .keyboardShortcut(.return, modifiers: .command)
                .padding(5)
                .accessibilityLabel(Text("Send"))
                .accessibilityIdentifier("terminal.composer.send")
            }
            .background { RaisedBackground(shape: shape, fill: theme.colors.lifted, elevation: .lifted) }
            .overlay {
                shape.strokeBorder(theme.colors.accent.opacity(focused ? 0.65 : 0), lineWidth: 1.5)
                    .allowsHitTesting(false)
            }
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
