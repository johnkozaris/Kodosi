import Foundation
import KodosiKit
import os

/// An event that the app cannot read, and the events that it drops when it falls behind, go
/// into the app's log, next to the lines of its link to the runtime, where a report can name
/// them. The app goes on without them.
public struct RuntimeEventFaults {
    private var last: String?

    public init() {}

    /// A fault that repeats with each new event goes into the log once.
    public mutating func unreadable(_ type: String?, _ error: any Error) {
        let fault = "the app could not read \(type.map { "the event \($0)" } ?? "an event"): \(Self.reason(error))"
        guard fault != last else { return }
        last = fault
        Self.log(fault, level: Int32(KODOSI_LOG_ERROR), type: .error)
    }

    public static func behind(_ drop: RuntimeEventInbox.Drop) {
        let milliseconds = drop.behindFor.components.seconds * 1000 + drop.behindFor.components.attoseconds / 1_000_000_000_000_000
        log("the app fell behind the runtime's events for \(milliseconds) ms and dropped \(drop.events) of them, \(drop.bytes) bytes"
            + " (\(drop.kinds)), so it asks for its state again", level: Int32(KODOSI_LOG_WARNING), type: .default)
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
