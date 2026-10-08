import CoreFoundation
import Foundation
import os

extension TerminalSessionManager {
    struct ControlFrameHandlers {
        let onClosed: (String?) -> Void
        let onInvalidControl: () -> Void
        let onFocusResult: (String, String, Bool) -> Void
        let onResizeApplied: (TerminalResizeIdentity) -> Void
        let onResizeRejected: (TerminalResizeIdentity) -> Void
        let onBell: () -> Void
    }

    static func handleControlFrame(
        _ data: Data,
        sessionId: String,
        handlers: ControlFrameHandlers
    ) {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = json["type"] as? String else { return }

        switch type {
        case "term.closed": handleClosed(json, sessionId: sessionId, handlers: handlers)
        case "term.focusApplied", "term.focusRejected":
            handleFocusResult(json, applied: type == "term.focusApplied", sessionId: sessionId, handlers: handlers)
        case "term.resizeApplied":
            guard let identity = terminalResizeIdentity(from: json, sessionId: sessionId) else { return }
            handlers.onResizeApplied(identity)
        case "term.resizeRejected":
            guard let identity = terminalResizeIdentity(from: json, sessionId: sessionId) else { return }
            handlers.onResizeRejected(identity)
        case "term.bell":
            guard controlSessionId(from: json) == sessionId else { return }
            handlers.onBell()
        default:
            Logger.terminal.debug("Terminal control '\(type)' for \(sessionId)")
        }
    }

    private static func handleFocusResult(
        _ json: [String: Any], applied: Bool, sessionId: String, handlers: ControlFrameHandlers
    ) {
        guard controlSessionId(from: json) == sessionId,
              let requestId = json["requestId"] as? String,
              let runtimeIncarnationId = json["runtimeIncarnationId"] as? String
        else { return }
        handlers.onFocusResult(requestId, runtimeIncarnationId, applied)
    }

    private static func handleClosed(_ json: [String: Any], sessionId: String, handlers: ControlFrameHandlers) {
        guard let finalSequence = terminalCloseBoundary(from: json) else {
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

    private nonisolated static func terminalResizeIdentity(
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

    private nonisolated static func exactUInt64(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID()
        else { return nil }
        return UInt64(number.stringValue)
    }

    private nonisolated static func exactUInt32(_ value: Any?) -> UInt32? {
        exactUInt64(value).flatMap(UInt32.init(exactly:))
    }

    private nonisolated static func exactUInt16(_ value: Any?) -> UInt16? {
        exactUInt64(value).flatMap(UInt16.init(exactly:))
    }

    nonisolated static func sanitizeTerminalTitle(_ title: String?) -> String? {
        guard let title else { return nil }
        let sanitized = title.unicodeScalars
            .lazy
            .filter {
                !CharacterSet.controlCharacters.contains($0)
                    && !isBidiControl($0.value)
            }
            .prefix(512)
            .reduce(into: "") { result, scalar in
                result.unicodeScalars.append(scalar)
            }
        return sanitized.isEmpty ? nil : sanitized
    }

    private nonisolated static func isBidiControl(_ value: UInt32) -> Bool {
        switch value {
        case 0x061C, 0x200E, 0x200F, 0x202A ... 0x202E, 0x2066 ... 0x2069:
            true
        default:
            false
        }
    }

    private nonisolated static func controlSessionId(from json: [String: Any]) -> String? {
        json["sessionId"] as? String
    }

    nonisolated static func terminalCloseBoundary(from json: [String: Any]) -> UInt64? {
        guard let boundary = json["finalSequence"] as? NSNumber,
              CFGetTypeID(boundary) != CFBooleanGetTypeID()
        else { return nil }
        return UInt64(boundary.stringValue)
    }
}
