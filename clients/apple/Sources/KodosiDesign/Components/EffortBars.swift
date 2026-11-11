import SwiftUI

public struct EffortBars: View {
    @Environment(\.theme) private var theme
    let level: Int?
    let count: Int
    let height: CGFloat
    let width: CGFloat

    public init(level: Int?, count: Int, height: CGFloat = 11, width: CGFloat = 2.5) {
        self.level = level
        self.count = count
        self.height = height
        self.width = width
    }

    public var body: some View {
        HStack(alignment: .bottom, spacing: width * 0.6) {
            ForEach(0 ..< count, id: \.self) { index in
                RoundedRectangle(cornerRadius: width * 0.4, style: .continuous)
                    .fill(level.map { index <= $0 } == true ? theme.colors.accent : theme.colors.ink.opacity(0.2))
                    .frame(width: width, height: height * (0.35 + 0.65 * CGFloat(index) / CGFloat(max(count - 1, 1))))
                    .animation(theme.motion.snappy, value: level)
            }
        }
        .frame(height: height, alignment: .bottom)
        .accessibilityHidden(true)
    }
}
