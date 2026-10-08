import Foundation
import GhosttyKit
import AppKit

func terminalClipboardRequestAllowed(
    request: ghostty_clipboard_request_e
) -> Bool {
    request == GHOSTTY_CLIPBOARD_REQUEST_PASTE
}

func terminalClipboardRequestsPlainText(
    mimeTypes: UnsafePointer<UnsafePointer<CChar>?>?,
    count: Int
) -> Bool {
    guard let mimeTypes, count > 0 else { return false }
    return (0..<count).contains { index in
        guard let mime = mimeTypes[index] else { return false }
        return String(cString: mime) == "text/plain"
    }
}

func terminalClipboardWriteAllowed(
    clipboard: ghostty_clipboard_e,
    requiresConfirmation: Bool
) -> Bool {
    clipboard == GHOSTTY_CLIPBOARD_STANDARD && !requiresConfirmation
}

private func copyTerminalAction(_ action: ghostty_action_s) -> TerminalActionSnapshot {
    switch action.tag {
    case GHOSTTY_ACTION_CELL_SIZE:
        .cellSize(
            width: action.action.cell_size.width,
            height: action.action.cell_size.height
        )
    case GHOSTTY_ACTION_RENDER:
        .render
    case GHOSTTY_ACTION_CONFIG_CHANGE:
        .configChange
    case GHOSTTY_ACTION_COMMAND_FINISHED:
        .commandFinished(
            exitCode: action.action.command_finished.exit_code < 0
                ? nil
                : Int(action.action.command_finished.exit_code),
            durationNanos: action.action.command_finished.duration
        )
    case GHOSTTY_ACTION_OPEN_URL:
        .openURL(
            kind: TerminalOpenURLKind(action.action.open_url.kind),
            url: copyTerminalBytes(
                action.action.open_url.url,
                length: Int(exactly: action.action.open_url.len) ?? 0
            ) ?? ""
        )
    case GHOSTTY_ACTION_MOUSE_OVER_LINK:
        .mouseOverLink(copyTerminalBytes(
            action.action.mouse_over_link.url,
            length: action.action.mouse_over_link.len
        ))
    case GHOSTTY_ACTION_PWD:
        .pwd(action.action.pwd.pwd.map(String.init(cString:)) ?? "")
    case GHOSTTY_ACTION_SELECTION_CHANGED:
        .selectionChanged
    default:
        .unsupported(TerminalDebugLog.describe(action.tag))
    }
}

private func copyTerminalBytes(
    _ pointer: UnsafePointer<CChar>?,
    length: Int
) -> String? {
    guard let pointer, length > 0 else { return nil }
    let bytes = UnsafeRawBufferPointer(start: pointer, count: length)
    return String(decoding: bytes, as: UTF8.self)
}

private enum TerminalCallbacks {
    static func wakeup(userdata: UnsafeMutableRawPointer?) {
        guard let userdata else { return }
        let controller = Unmanaged<TerminalController>.fromOpaque(userdata)
            .takeUnretainedValue()
        terminalRunOnMain {
            controller.handleWakeup()
        }
    }

    static func action(
        appPtr: ghostty_app_t?,
        target: ghostty_target_s,
        action: ghostty_action_s
    ) -> Bool {
        guard let appPtr else { return false }
        guard ghostty_app_userdata(appPtr) != nil else { return false }
        guard target.tag == GHOSTTY_TARGET_SURFACE else { return false }
        guard let surfacePtr = target.target.surface else { return false }
        guard let bridgePtr = ghostty_surface_userdata(surfacePtr) else { return false }

        let bridge = Unmanaged<TerminalCallbackBridge>
            .fromOpaque(bridgePtr)
            .takeUnretainedValue()
        let snapshot = copyTerminalAction(action)
        Task { @MainActor in
            bridge.handleAction(snapshot)
        }

        return false
    }

    static func closeSurface(
        userdata: UnsafeMutableRawPointer?,
        processAlive: Bool
    ) {
        guard let userdata else { return }
        let bridge = Unmanaged<TerminalCallbackBridge>
            .fromOpaque(userdata)
            .takeUnretainedValue()
        terminalRunOnMain {
            bridge.handleClose(processAlive: processAlive)
        }
    }

    static func writeClipboard(
        userdata _: UnsafeMutableRawPointer?,
        clipboard: ghostty_clipboard_e,
        contents: UnsafePointer<ghostty_clipboard_content_s>?,
        contentsLen: Int,
        confirm: Bool
    ) {
        guard terminalClipboardWriteAllowed(
            clipboard: clipboard,
            requiresConfirmation: confirm
        ) else {
            TerminalDebugLog.log(.input, "host-managed clipboard write denied")
            return
        }
        guard let contents, contentsLen > 0 else { return }
        guard let content = (0..<contentsLen)
            .map({ contents[$0] })
            .first(where: { content in
                content.mime.map { String(cString: $0) == "text/plain" } ?? false
            }),
            let data = content.data
        else { return }
        let bytes = UnsafeRawBufferPointer(
            start: data,
            count: content.len
        )
        let string = String(decoding: bytes, as: UTF8.self)

        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(string, forType: .string)
    }

    static func readClipboard(
        userdata: UnsafeMutableRawPointer?,
        clipboard _: ghostty_clipboard_e,
        opaquePtr: UnsafeMutableRawPointer?,
        mimeTypes: UnsafePointer<UnsafePointer<CChar>?>?,
        mimeTypesLen: Int,
        list _: Bool
    ) -> ghostty_clipboard_read_result_e {
        guard let userdata, let opaquePtr else {
            return GHOSTTY_CLIPBOARD_READ_UNAVAILABLE
        }

        let bridge = Unmanaged<TerminalCallbackBridge>
            .fromOpaque(userdata)
            .takeUnretainedValue()
        guard let surface = bridge.rawSurface else {
            return GHOSTTY_CLIPBOARD_READ_UNAVAILABLE
        }
        let request = ghostty_clipboard_request_type(opaquePtr)
        if !terminalClipboardRequestAllowed(request: request) {
            TerminalDebugLog.log(.input, "host-managed clipboard read denied")
            return GHOSTTY_CLIPBOARD_READ_UNSUPPORTED
        }
        let string = TerminalPasteboardContent.text()
        guard let string else {
            TerminalDebugLog.log(.input, "clipboard paste read empty")
            return GHOSTTY_CLIPBOARD_READ_UNAVAILABLE
        }

        guard request == GHOSTTY_CLIPBOARD_REQUEST_PASTE,
              terminalClipboardRequestsPlainText(
                  mimeTypes: mimeTypes,
                  count: mimeTypesLen
              )
        else {
            TerminalDebugLog.log(.input, "clipboard paste requested unsupported MIME type")
            return GHOSTTY_CLIPBOARD_READ_UNSUPPORTED
        }

        TerminalDebugLog.log(
            .input,
            "clipboard paste read bytes=\(string.utf8.count) lines=\(TerminalInputText.lineCount(in: string))"
        )
        "text/plain".withCString { mime in
            string.withCString { data in
                var content = ghostty_clipboard_content_s(
                    mime: mime,
                    data: data,
                    len: string.utf8.count
                )
                withUnsafePointer(to: &content) { contentPointer in
                    var completion = ghostty_clipboard_complete_s(
                        contents: contentPointer,
                        contents_len: 1,
                        available: nil,
                        available_len: 0,
                        confirmed: false,
                        remember: false
                    )
                    ghostty_surface_complete_clipboard_request(
                        surface,
                        &completion,
                        opaquePtr
                    )
                }
            }
        }
        TerminalDebugLog.log(.input, "clipboard paste complete")
        return GHOSTTY_CLIPBOARD_READ_STARTED
    }

    static func confirmReadClipboard(
        userdata: UnsafeMutableRawPointer?,
        confirmation: UnsafePointer<ghostty_clipboard_confirm_s>?,
        opaquePtr: UnsafeMutableRawPointer?,
        request: ghostty_clipboard_request_e
    ) {
        guard let userdata, let opaquePtr else { return }

        let bridge = Unmanaged<TerminalCallbackBridge>
            .fromOpaque(userdata)
            .takeUnretainedValue()
        guard let surface = bridge.rawSurface else { return }
        guard terminalClipboardRequestAllowed(request: request),
              let confirmation
        else {
            TerminalDebugLog.log(.input, "host-managed clipboard confirmation denied")
            ghostty_surface_deny_clipboard_request(surface, opaquePtr)
            return
        }

        let value = confirmation.pointee
        let textBytes = value.contents.flatMap { contents in
            guard value.contents_len > 0,
                  let data = contents.pointee.data
            else { return nil as UnsafeRawBufferPointer? }
            return UnsafeRawBufferPointer(
                start: data,
                count: contents.pointee.len
            )
        }
        let text = textBytes.map { String(decoding: $0, as: UTF8.self) } ?? ""
        TerminalDebugLog.log(
            .input,
            "clipboard paste confirm request=\(request.rawValue) bytes=\(text.utf8.count) lines=\(TerminalInputText.lineCount(in: text))"
        )
        var completion = ghostty_clipboard_complete_s(
            contents: value.contents,
            contents_len: value.contents_len,
            available: value.available,
            available_len: value.available_len,
            confirmed: true,
            remember: false
        )
        ghostty_surface_complete_clipboard_request(
            surface,
            &completion,
            opaquePtr
        )
        TerminalDebugLog.log(.input, "clipboard paste confirmed")
    }
}

func terminalControllerWakeupCallback(userdata: UnsafeMutableRawPointer?) {
    TerminalCallbacks.wakeup(userdata: userdata)
}

func terminalControllerActionCallback(
    appPtr: ghostty_app_t?,
    target: ghostty_target_s,
    action: ghostty_action_s
) -> Bool {
    TerminalCallbacks.action(appPtr: appPtr, target: target, action: action)
}

func terminalControllerCloseSurfaceCallback(
    userdata: UnsafeMutableRawPointer?,
    processAlive: Bool
) {
    TerminalCallbacks.closeSurface(userdata: userdata, processAlive: processAlive)
}

func terminalControllerWriteClipboardCallback(
    userdata: UnsafeMutableRawPointer?,
    clipboard: ghostty_clipboard_e,
    contents: UnsafePointer<ghostty_clipboard_content_s>?,
    contentsLen: Int,
    confirm: Bool
) {
    TerminalCallbacks.writeClipboard(
        userdata: userdata,
        clipboard: clipboard,
        contents: contents,
        contentsLen: contentsLen,
        confirm: confirm
    )
}

func terminalControllerReadClipboardCallback(
    userdata: UnsafeMutableRawPointer?,
    clipboard: ghostty_clipboard_e,
    opaquePtr: UnsafeMutableRawPointer?,
    mimeTypes: UnsafePointer<UnsafePointer<CChar>?>?,
    mimeTypesLen: Int,
    list: Bool
) -> ghostty_clipboard_read_result_e {
    TerminalCallbacks.readClipboard(
        userdata: userdata,
        clipboard: clipboard,
        opaquePtr: opaquePtr,
        mimeTypes: mimeTypes,
        mimeTypesLen: mimeTypesLen,
        list: list
    )
}

func terminalControllerConfirmReadClipboardCallback(
    userdata: UnsafeMutableRawPointer?,
    confirmation: UnsafePointer<ghostty_clipboard_confirm_s>?,
    opaquePtr: UnsafeMutableRawPointer?,
    request: ghostty_clipboard_request_e
) {
    TerminalCallbacks.confirmReadClipboard(
        userdata: userdata,
        confirmation: confirmation,
        opaquePtr: opaquePtr,
        request: request
    )
}
