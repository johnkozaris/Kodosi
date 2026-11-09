import SwiftUI

public extension View {
    /// The field of a composer: lifted on its ground, more while it has focus, with a copper ring
    /// then.
    func composerField(focused: Bool) -> some View {
        modifier(ComposerField(focused: focused))
    }
}

private struct ComposerField: ViewModifier {
    @Environment(\.theme) private var theme
    let focused: Bool

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: 20, style: .continuous)
        content
            .padding(5)
            .background { RaisedBackground(shape: shape, fill: theme.colors.lifted, elevation: focused ? .lifted : .resting) }
            .overlay {
                shape.strokeBorder(theme.colors.accent.mix(with: theme.colors.lifted, by: 0.5), lineWidth: 1.5)
                    .opacity(focused ? 1 : 0)
                    .allowsHitTesting(false)
            }
    }
}

/// The key that sends what a composer holds: copper when there is something to send, and lit
/// around its edge while it sends.
public struct SendKey: View {
    @Environment(\.theme) private var theme
    let enabled: Bool
    let working: Bool
    let size: CGFloat
    let action: () -> Void

    public init(enabled: Bool, working: Bool = false, size: CGFloat = 30, action: @escaping () -> Void) {
        self.enabled = enabled
        self.working = working
        self.size = size
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            Image(systemName: "arrow.up").font(.system(size: (size * 0.43).rounded(), weight: .bold))
                .foregroundStyle(enabled ? theme.colors.onAccent : theme.colors.inkFaint)
                .frame(width: size, height: size)
                .background {
                    if enabled {
                        BezelBackground(shape: Circle(), fill: theme.colors.accent)
                    } else {
                        Circle().fill(theme.colors.lifted.mix(with: theme.colors.ink, by: 0.08))
                    }
                }
                .overlay {
                    if working {
                        LightRim(.working, lineWidth: 2).padding(-4)
                    }
                }
                .touchTarget()
        }
        .buttonStyle(PressScaleStyle()).disabled(!enabled)
    }
}

/// A short note about something that did not work, with a key that dismisses it.
public struct CautionNote<Actions: View>: View {
    @Environment(\.theme) private var theme
    let message: String
    let lines: Int
    let identifier: String
    let dismiss: () -> Void
    let actions: Actions

    public init(
        message: String, lines: Int = 2, identifier: String, dismiss: @escaping () -> Void, @ViewBuilder actions: () -> Actions,
    ) {
        self.message = message
        self.lines = lines
        self.identifier = identifier
        self.dismiss = dismiss
        self.actions = actions()
    }

    public var body: some View {
        HStack(spacing: 8) {
            Circle().fill(theme.colors.caution).frame(width: 6, height: 6).accessibilityHidden(true)
            Text(message).appTextStyle(.footnote).foregroundStyle(theme.colors.ink).lineLimit(lines)
            Spacer(minLength: 4)
            actions
            IconButton(title: "Dismiss", bundle: .module, symbol: "xmark", identifier: "\(identifier).dismiss", size: 22, action: dismiss)
        }
        .padding(.leading, 12).padding(.trailing, 4).padding(.vertical, 6)
        .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }
}

public extension CautionNote where Actions == EmptyView {
    init(message: String, lines: Int = 2, identifier: String, dismiss: @escaping () -> Void) {
        self.init(message: message, lines: lines, identifier: identifier, dismiss: dismiss) { EmptyView() }
    }
}
