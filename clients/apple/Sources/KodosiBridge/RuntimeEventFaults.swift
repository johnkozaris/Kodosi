import Foundation
import KodosiKit
import os

/// Logs undecodable or dropped runtime events to both `os.Logger` and the runtime log so
/// diagnostic reports include them while the app continues running.
public struct RuntimeEventFaults {
    private var last: String?

    public init() {}

    public mutating func unreadable(_ type: String?, _ error: any Error) {
        let fault = "the app could not read \(type.map { "the event \($0)" } ?? "an event"): \(Self.reason(error))"
        guard fault != last else { return }
        last = fault
        Self.log(fault, level: Int32(KODOSI_LOG_ERROR), type: .error)
    }

    public static func behind(_ drop: RuntimeEventInbox.Drop) {
        log(dropped(drop, by: "the app fell behind the runtime's events") + ", so it asks for its state again",
            level: Int32(KODOSI_LOG_WARNING), type: .default)
    }

    static func videoBehind(_ drop: RuntimeEventInbox.Drop) {
        log(dropped(drop, by: "the video pipes fell behind their frames") + ", so its pipes start again",
            level: Int32(KODOSI_LOG_WARNING), type: .default)
    }

    private static func dropped(_ drop: RuntimeEventInbox.Drop, by fell: String) -> String {
        let milliseconds = drop.behindFor.components.seconds * 1000 + drop.behindFor.components.attoseconds / 1_000_000_000_000_000
        return "\(fell) for \(milliseconds) ms and dropped \(drop.events) of them, \(drop.bytes) bytes (\(drop.kinds))"
    }

    private static func reason(_ error: any Error) -> String {
        guard let error = error as? DecodingError else { return String(describing: error) }
        var path: [any CodingKey] = []
        let context: DecodingError.Context
        switch error {
        case let .typeMismatch(_, found), let .valueNotFound(_, found), let .dataCorrupted(found):
            context = found
        case let .keyNotFound(key, found):
            context = found
            path = [key]
        @unknown default:
            return String(describing: error)
        }
        let place = (context.codingPath + path).map { $0.intValue.map(String.init) ?? $0.stringValue }.joined(separator: ".")
        return place.isEmpty ? context.debugDescription : "\(place): \(context.debugDescription)"
    }

    private static func log(_ line: String, level: Int32, type: OSLogType) {
        Logger.runtime.log(level: type, "\(line, privacy: .public)")
        var line = line
        line.withUTF8 { _ = kodosi_log(level, $0.baseAddress, UInt($0.count)) }
    }
}
