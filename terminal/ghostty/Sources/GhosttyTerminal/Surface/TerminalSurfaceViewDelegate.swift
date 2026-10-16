import GhosttyKit

@MainActor
public protocol TerminalSurfaceViewDelegate: AnyObject {}

@MainActor
public protocol TerminalSurfaceGridResizeDelegate: TerminalSurfaceViewDelegate {
    func terminalDidResize(_ size: TerminalGridMetrics)
}

public enum TerminalOpenURLKind: Sendable {
    case unknown
    case text
    case html

    init(_ raw: ghostty_action_open_url_kind_e) {
        switch raw {
        case GHOSTTY_ACTION_OPEN_URL_KIND_TEXT: self = .text
        case GHOSTTY_ACTION_OPEN_URL_KIND_HTML: self = .html
        default: self = .unknown
        }
    }
}

@MainActor
public protocol TerminalSurfaceOpenURLDelegate: TerminalSurfaceViewDelegate {
    func terminalDidRequestOpenURL(_ url: String, kind: TerminalOpenURLKind)
}

@MainActor
public protocol TerminalSurfaceHoverLinkDelegate: TerminalSurfaceViewDelegate {
    func terminalDidUpdateHoverLink(_ url: String?)
}

@MainActor
public protocol TerminalSurfaceClipboardReadDelegate: TerminalSurfaceViewDelegate {
    func terminalDidRequestClipboardRead(_ answer: @escaping @MainActor (Bool) -> Void)
}

@MainActor
public protocol TerminalSurfaceLifecycleDelegate: TerminalSurfaceViewDelegate {
    func terminalDidAttachSurface(_ surface: TerminalSurface)
    func terminalDidDetachSurface()
}
