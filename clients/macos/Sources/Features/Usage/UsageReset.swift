import KodosiBridge
import KodosiDesign
import SwiftUI

enum ResetTime {
    static func left(_ limit: UsageLimit, now: Date = .now) -> TimeInterval? {
        guard let resets = limit.resetsAt, resets > now else { return nil }
        return resets.timeIntervalSince(now)
    }

    static func short(_ left: TimeInterval, locale: Locale = .autoupdatingCurrent) -> String {
        format(left, width: .narrow, locale: locale)
    }

    static func spoken(_ left: TimeInterval, locale: Locale = .autoupdatingCurrent) -> String {
        format(left, width: .wide, locale: locale)
    }

    private static func format(_ left: TimeInterval, width: Duration.UnitsFormatStyle.UnitWidth, locale: Locale) -> String {
        let minutes = max(1, (left / 60).rounded(.up))
        let (rounded, units): (TimeInterval, Set<Duration.UnitsFormatStyle.Unit>) = switch minutes {
        case (24 * 60)...: ((left / 3600).rounded(.up) * 3600, [.days, .hours])
        case 60...: (minutes * 60, [.hours, .minutes])
        default: (minutes * 60, [.minutes])
        }
        return Duration.seconds(rounded).formatted(.units(allowed: units, width: width).locale(locale))
    }
}

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
