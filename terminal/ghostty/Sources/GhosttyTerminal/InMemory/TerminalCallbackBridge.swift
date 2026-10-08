import Foundation
import GhosttyKit

enum TerminalActionSnapshot: Sendable {
    case cellSize(width: UInt32, height: UInt32)
    case render
    case configChange
    case commandFinished(exitCode: Int?, durationNanos: UInt64)
    case openURL(kind: TerminalOpenURLKind, url: String)
    case mouseOverLink(String?)
    case pwd(String)
    case selectionChanged
    case unsupported(String)
}

@MainActor
final class TerminalCallbackBridge {
    weak var delegate: (any TerminalSurfaceViewDelegate)?
    nonisolated(unsafe) var rawSurface: ghostty_surface_t?
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

        case let .commandFinished(exit, duration):
            TerminalDebugLog.log(
                .actions,
                "callback action=command_finished exit=\(exit.map { "\($0)" } ?? "nil") duration_ns=\(duration)"
            )
            (delegate as? any TerminalSurfaceCommandFinishedDelegate)?
                .terminalDidFinishCommand(exitCode: exit, durationNanos: duration)

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

        case let .pwd(pwd):
            TerminalDebugLog.log(
                .actions,
                "callback action=pwd pwd=\(TerminalDebugLog.describe(pwd))"
            )
            (delegate as? any TerminalSurfacePwdDelegate)?
                .terminalDidChangeWorkingDirectory(pwd)

        case .selectionChanged:
            TerminalDebugLog.log(.actions, "callback action=selection_changed")
            onSelectionChanged?()

        case let .unsupported(tag):
            TerminalDebugLog.log(.actions, "callback action=\(tag)")
        }
    }

    func handleClose(processAlive: Bool) {
        TerminalDebugLog.log(
            .lifecycle,
            "callback close processAlive=\(processAlive)"
        )
        (delegate as? any TerminalSurfaceCloseDelegate)?
            .terminalDidClose(processAlive: processAlive)
    }
}
