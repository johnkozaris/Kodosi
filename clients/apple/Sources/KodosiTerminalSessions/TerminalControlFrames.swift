import CoreFoundation
import Foundation
import KodosiBridge
import os

struct TerminalControlHandlers {
    let onClosed: (String?) -> Void
    let onInvalidControl: () -> Void
    let onFocusResult: (String, String, Bool) -> Void
    let onResizeApplied: (TerminalResizeIdentity) -> Void
    let onResizeRejected: (TerminalResizeIdentity, String?) -> Void
    let onBell: () -> Void
}

enum TerminalControlFrames {
    static func handle(
        _ data: Data,
        sessionId: String,
        handlers: TerminalControlHandlers
    ) {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = json["type"] as? String else { return }

        switch type {
        case "term.closed": handleClosed(json, sessionId: sessionId, handlers: handlers)
        case "term.focusApplied", "term.focusRejected":
            handleFocusResult(json, applied: type == "term.focusApplied", sessionId: sessionId, handlers: handlers)
        case "term.resizeApplied":
            guard let identity = resizeIdentity(from: json, sessionId: sessionId) else { return }
            handlers.onResizeApplied(identity)
        case "term.resizeRejected":
            guard let identity = resizeIdentity(from: json, sessionId: sessionId) else { return }
            handlers.onResizeRejected(identity, json["reason"] as? String)
        case "term.bell":
            guard controlSessionId(from: json) == sessionId else { return }
            handlers.onBell()
        default:
            Logger.terminal.debug("Terminal control '\(type)' for \(sessionId)")
        }
    }

    static func closeBoundary(from json: [String: Any]) -> UInt64? {
        guard let boundary = json["finalSequence"] as? NSNumber,
              CFGetTypeID(boundary) != CFBooleanGetTypeID()
        else { return nil }
        return UInt64(boundary.stringValue)
    }

    private static func handleFocusResult(
        _ json: [String: Any], applied: Bool, sessionId: String, handlers: TerminalControlHandlers
    ) {
        guard controlSessionId(from: json) == sessionId,
              let requestId = json["requestId"] as? String,
              let runtimeIncarnationId = json["runtimeIncarnationId"] as? String
        else { return }
        handlers.onFocusResult(requestId, runtimeIncarnationId, applied)
    }

    private static func handleClosed(_ json: [String: Any], sessionId: String, handlers: TerminalControlHandlers) {
        guard let finalSequence = closeBoundary(from: json) else {
            Logger.terminal.error("Invalid terminal close boundary for \(sessionId)")
            handlers.onInvalidControl()
            return
        }
        let reason = json["reason"] as? String
        handlers.onClosed(reason)
        Logger.terminal.info(
            "Terminal closed for \(sessionId) at sequence \(finalSequence): \(reason ?? "unknown")"
        )
    }

    private static func resizeIdentity(
        from json: [String: Any],
        sessionId: String
    ) -> TerminalResizeIdentity? {
        guard controlSessionId(from: json) == sessionId,
              let requestId = json["requestId"] as? String,
              let expectedRuntimeIncarnationId = json["expectedRuntimeIncarnationId"] as? String,
              let subscriptionId = json["subscriptionId"] as? String,
              let subscriptionGeneration = exactUInt64(json["subscriptionGeneration"]),
              let surfaceGeneration = exactUInt64(json["surfaceGeneration"]),
              let cols = exactUInt16(json["cols"]),
              let rows = exactUInt16(json["rows"]),
              let widthPixels = exactUInt32(json["widthPixels"]),
              let heightPixels = exactUInt32(json["heightPixels"]),
              let cellWidthPixels = exactUInt32(json["cellWidthPixels"]),
              let cellHeightPixels = exactUInt32(json["cellHeightPixels"]),
              cols > 0,
              rows > 0,
              widthPixels == UInt32(cols) * cellWidthPixels,
              heightPixels == UInt32(rows) * cellHeightPixels
        else { return nil }
        return TerminalResizeIdentity(
            requestId: requestId,
            expectedRuntimeIncarnationId: expectedRuntimeIncarnationId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration,
            surfaceGeneration: surfaceGeneration,
            cols: cols,
            rows: rows,
            widthPixels: widthPixels,
            heightPixels: heightPixels,
            cellWidthPixels: cellWidthPixels,
            cellHeightPixels: cellHeightPixels
        )
    }

    private static func exactUInt64(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID()
        else { return nil }
        return UInt64(number.stringValue)
    }

    private static func exactUInt32(_ value: Any?) -> UInt32? {
        exactUInt64(value).flatMap(UInt32.init(exactly:))
    }

    private static func exactUInt16(_ value: Any?) -> UInt16? {
        exactUInt64(value).flatMap(UInt16.init(exactly:))
    }

    private static func controlSessionId(from json: [String: Any]) -> String? {
        json["sessionId"] as? String
    }
}
