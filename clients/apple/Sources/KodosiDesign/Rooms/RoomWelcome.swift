import SwiftUI

public struct RoomWelcome: View {
    public struct Step: Identifiable {
        public let title: LocalizedStringKey
        public let symbol: String
        public let done: Bool
        public let action: () -> Void

        public var id: String {
            symbol
        }

        public init(title: LocalizedStringKey, symbol: String, done: Bool, action: @escaping () -> Void) {
            self.title = title
            self.symbol = symbol
            self.done = done
            self.action = action
        }
    }

    @Environment(\.theme) private var theme
    let name: String
    let key: String
    let steps: [Step]

    public init(name: String, key: String, steps: [Step]) {
        self.name = name
        self.key = key
        self.steps = steps
    }

    public var body: some View {
        VStack(spacing: 18) {
            RoomSigil(key: key, size: 56).arrive()
            VStack(spacing: 4) {
                Text("This is \(name)", bundle: .module)
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.center)
                Text("Say hello, or start with one of these.", bundle: .module)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            .arrive(delay: 0.04)
            VStack(spacing: 8) {
                ForEach(steps) { step($0) }
            }
            .arrive(delay: 0.08)
        }
        .padding(20)
        .frame(maxWidth: 340)
    }

    private func step(_ step: Step) -> some View {
        Button(action: step.action) {
            HStack(spacing: 12) {
                IconTile(symbol: step.symbol, tint: step.done ? TileTint.graphite : TileTint.copper, size: 30)
                Text(step.title)
                    .appTextStyle(.body)
                    .foregroundStyle(step.done ? theme.colors.inkMuted : theme.colors.ink)
                Spacer(minLength: 4)
                Image(systemName: step.done ? "checkmark.circle.fill" : "chevron.right")
                    .font(.system(size: step.done ? 17 : 12, weight: .semibold))
                    .foregroundStyle(step.done ? theme.colors.ready : theme.colors.inkFaint)
                    .contentTransition(.symbolEffect(.replace))
            }
            .padding(.horizontal, 12)
            .frame(minHeight: 50)
            .raised(Radius.lg, fill: theme.colors.lifted)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
    }
}
