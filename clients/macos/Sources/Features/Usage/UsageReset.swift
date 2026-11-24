import KodosiBridge
import KodosiDesign
import SwiftUI

struct UsageReset: View {
    @Environment(\.theme) private var theme
    let limit: UsageLimit?

    var body: some View {
        Text(verbatim: limit.flatMap { ResetTime.left($0) }.map { ResetTime.short($0) } ?? "")
            .appTextStyle(.caption2).monospacedDigit()
            .foregroundStyle(theme.colors.inkFaint)
            .lineLimit(1).minimumScaleFactor(0.75)
            .frame(height: 12)
    }
}

extension UsageLimit {
    var spokenWithReset: String {
        guard let left = ResetTime.left(self) else { return spoken }
        return String(localized: "\(spoken), resets in \(ResetTime.spoken(left))")
    }
}
