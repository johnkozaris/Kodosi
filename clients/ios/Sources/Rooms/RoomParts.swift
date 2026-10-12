import Foundation
import KodosiBridge
import KodosiDesign
import SwiftUI

enum RoomDates {
    static func parse(_ value: String) -> Date? {
        (try? Date(value, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)))
            ?? (try? Date(value, strategy: .iso8601))
    }

    static func day(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return String(localized: "Today")
        }
        if calendar.isDateInYesterday(date) {
            return String(localized: "Yesterday")
        }
        return date.formatted(.dateTime.weekday(.wide).month().day())
    }

    static func short(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }
        if calendar.isDateInYesterday(date) {
            return String(localized: "Yesterday")
        }
        if let days = calendar.dateComponents([.day], from: date, to: .now).day, days < 7 {
            return date.formatted(.dateTime.weekday(.abbreviated))
        }
        return date.formatted(.dateTime.month(.abbreviated).day())
    }

    static func startsGroup(_ message: RoomMessage, after previous: RoomMessage?) -> Bool {
        guard let previous, previous.authorId == message.authorId, previous.agent == message.agent,
              previous.terminalId == message.terminalId
        else { return true }
        guard let date = parse(message.createdAt), let before = parse(previous.createdAt) else { return false }
        return date.timeIntervalSince(before) > 300 || !Calendar.current.isDate(date, inSameDayAs: before)
    }

    static func startsDay(_ message: RoomMessage, after previous: RoomMessage?) -> Bool {
        guard let date = parse(message.createdAt) else { return false }
        guard let previous, let before = parse(previous.createdAt) else { return true }
        return !Calendar.current.isDate(date, inSameDayAs: before)
    }
}

enum RoomWords {
    static func people(_ count: Int) -> String {
        count == 1 ? String(localized: "1 person") : String(localized: "\(count) people")
    }

    static func terminals(_ count: Int) -> String {
        count == 1 ? String(localized: "1 terminal") : String(localized: "\(count) terminals")
    }

    static func name(_ member: MissionMemberEntry) -> String {
        member.displayName.isEmpty ? member.handle : member.displayName
    }

    static func name(_ friend: FriendEntry) -> String {
        friend.displayName.isEmpty ? friend.handle : friend.displayName
    }
}

struct RoomSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            ForEach(0 ..< 4, id: \.self) { index in
                HStack(alignment: .top, spacing: 12) {
                    SkeletonBlock(width: 30, height: 30, radius: 15)
                    VStack(alignment: .leading, spacing: 8) {
                        SkeletonBlock(width: 96, height: 10)
                        SkeletonBlock(height: 11)
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 170 : 240, height: 11)
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Loading"))
    }
}

struct RoomRecovery: View {
    @Environment(\.theme) private var theme
    let message: String
    let retry: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "wifi.exclamationmark")
                .font(.system(size: 22, weight: .medium))
                .foregroundStyle(theme.colors.caution)
                .frame(width: 56, height: 56)
                .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: 17, style: .continuous))
                .accessibilityHidden(true)
                .arrive()
            VStack(spacing: 6) {
                Text("This did not load")
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                Text(verbatim: message)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }
            .arrive(delay: 0.05)
            Button("Try again", action: retry)
                .buttonStyle(.kodosi(.primary))
                .accessibilityIdentifier("rooms.retry")
                .arrive(delay: 0.1)
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

nonisolated struct RoomFlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        arrange(width: proposal.width ?? .infinity, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        let result = arrange(width: bounds.width, subviews: subviews)
        for (index, origin) in result.origins.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), anchor: .topLeading, proposal: .unspecified)
        }
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> (size: CGSize, origins: [CGPoint]) {
        var origins: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var line: CGFloat = 0
        var widest: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += line + spacing
                line = 0
            }
            origins.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            line = max(line, size.height)
            widest = max(widest, x - spacing)
        }
        return (CGSize(width: widest, height: y + line), origins)
    }
}
