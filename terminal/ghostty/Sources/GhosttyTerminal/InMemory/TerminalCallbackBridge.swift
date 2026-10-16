import Foundation
import GhosttyKit

enum TerminalActionSnapshot: Sendable {
    case cellSize(width: UInt32, height: UInt32)
    case render
    case configChange
    case openURL(kind: TerminalOpenURLKind, url: String)
    case mouseOverLink(String?)
    case selectionChanged
    case unsupported(String)
}

@MainActor
final class TerminalCallbackBridge {
    weak var delegate: (any TerminalSurfaceViewDelegate)?
    nonisolated(unsafe) var rawSurface: ghostty_surface_t?
    nonisolated(unsafe) var hasFocus = false
    nonisolated(unsafe) weak var session: InMemoryTerminalSession?
    var onCellSizeChange: ((UInt32, UInt32) -> Void)?
    var onRenderRequest: (() -> Void)?
    var onSelectionChanged: (() -> Void)?
    var onAppWakeup: (() -> Void)?
    var canProcessAppWakeup: (() -> Bool)?
    var surfaceConfig: ghostty_config_t?
    var managedConfigURL: URL?

    init(delegate: (any TerminalSurfaceViewDelegate)? = nil) {
        self.delegate = delegate
    }

    func clearSurfaceConfig() {
        if let surfaceConfig {
            ghostty_config_free(surfaceConfig)
            self.surfaceConfig = nil
        }
        if let managedConfigURL {
            try? FileManager.default.removeItem(at: managedConfigURL)
            self.managedConfigURL = nil
        }
    }

    func askToShareClipboard() {
        guard let delegate = delegate as? any TerminalSurfaceClipboardReadDelegate else { return }
        delegate.terminalDidRequestClipboardRead { [weak self] allowed in
            // A refused program gets an empty answer, so it does not wait for one.
            self?.session?.sendInput(
                terminalClipboardReport(allowed ? TerminalPasteboardContent.plainText() ?? "" : "")
            )
        }
    }

    func handleAction(_ action: TerminalActionSnapshot) {
        switch action {
        case let .cellSize(width, height):
            TerminalDebugLog.log(
                .actions,
                "callback action=cell_size width=\(width) height=\(height)"
            )
            onCellSizeChange?(width, height)

        case .render:
            TerminalDebugLog.log(.render, "callback action=render")
            onRenderRequest?()

        case .configChange:
            TerminalDebugLog.log(.actions, "callback action=config_change")
            onRenderRequest?()

        case let .openURL(kind, url):
            TerminalDebugLog.log(
                .actions,
                "callback action=open_url kind=\(kind) url=\(TerminalDebugLog.describe(url))"
            )
            (delegate as? any TerminalSurfaceOpenURLDelegate)?
                .terminalDidRequestOpenURL(url, kind: kind)

        case let .mouseOverLink(url):
            TerminalDebugLog.log(
                .actions,
                "callback action=mouse_over_link url=\(url.map { TerminalDebugLog.describe($0) } ?? "nil")"
            )
            (delegate as? any TerminalSurfaceHoverLinkDelegate)?
                .terminalDidUpdateHoverLink(url)

        case .selectionChanged:
            TerminalDebugLog.log(.actions, "callback action=selection_changed")
            onSelectionChanged?()

        case let .unsupported(tag):
            TerminalDebugLog.log(.actions, "callback action=\(tag)")
        }
    }
}
