import Foundation

public enum RoomDates {
    public static func parse(_ value: String) -> Date? {
        (try? Date(value, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)))
            ?? (try? Date(value, strategy: .iso8601))
    }

    public static func day(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return String(localized: "Today", bundle: .module)
        }
        if calendar.isDateInYesterday(date) {
            return String(localized: "Yesterday", bundle: .module)
        }
        return date.formatted(.dateTime.weekday(.wide).month().day())
    }

    public static func short(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }
        if calendar.isDateInYesterday(date) {
            return String(localized: "Yesterday", bundle: .module)
        }
        if let days = calendar.dateComponents([.day], from: date, to: .now).day, days < 7 {
            return date.formatted(.dateTime.weekday(.abbreviated))
        }
        return date.formatted(.dateTime.month(.abbreviated).day())
    }

    public static func startsGroup(_ message: RoomMessage, after previous: RoomMessage?) -> Bool {
        guard let previous, previous.authorId == message.authorId, previous.agent == message.agent,
              previous.terminalId == message.terminalId
        else { return true }
        guard let date = parse(message.createdAt), let before = parse(previous.createdAt) else { return false }
        return date.timeIntervalSince(before) > 300 || !Calendar.current.isDate(date, inSameDayAs: before)
    }

    public static func startsDay(_ message: RoomMessage, after previous: RoomMessage?) -> Bool {
        guard let date = parse(message.createdAt) else { return false }
        guard let previous, let before = parse(previous.createdAt) else { return true }
        return !Calendar.current.isDate(date, inSameDayAs: before)
    }
}
