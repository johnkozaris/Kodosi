#if os(macOS)
    import AppKit
#elseif os(iOS)
    import UIKit
#endif
import Foundation
import GhosttyTerminal
import KodosiLinks

@MainActor
final class TerminalRendererDelegateProxy:
    TerminalSurfaceGridResizeDelegate,
    TerminalSurfaceLifecycleDelegate,
    TerminalSurfaceOpenURLDelegate,
    TerminalSurfaceClipboardReadDelegate,
    TerminalSurfacePictureDelegate
{
    weak var target: TerminalViewState?
    var viewportDidResize: (() -> Void)?
    var clipboardReadRequested: ((@escaping @MainActor (Bool) -> Void) -> Void)?
    var picturePasted: ((Data) -> Void)?
    var asksBeforeClipboardRead = true
    var otherComputer: String?
    var linkReviewRequested: ((ClickedLink) -> Void)?
    var opensLink: (URL) -> Void = { url in
        #if os(macOS)
            NSWorkspace.shared.open(url)
        #elseif os(iOS)
            UIApplication.shared.open(url)
        #endif
    }

    private var updateGeneration: UInt64 = 0
    private var isDeferring = false
    private var deferredCallbacks: [@MainActor () -> Void] = []

    func beginViewUpdate() {
        updateGeneration &+= 1
        isDeferring = true
    }

    func endViewUpdate() {
        let generation = updateGeneration
        Task { [weak self] in
            await Task.yield()
            guard let self, updateGeneration == generation else { return }
            isDeferring = false
            let callbacks = deferredCallbacks
            deferredCallbacks.removeAll(keepingCapacity: true)
            for callback in callbacks {
                callback()
            }
        }
    }

    func terminalDidResize(_ size: TerminalGridMetrics) {
        forward {
            $0.terminalDidResize(size)
            self.viewportDidResize?()
        }
    }

    func terminalDidAttachSurface(_ surface: TerminalSurface) {
        target?.terminalDidAttachSurface(surface)
    }

    func terminalDidDetachSurface() {
        forward { $0.terminalDidDetachSurface() }
    }

    func terminalDidRequestOpenURL(_ url: String, kind: TerminalOpenURLKind) {
        // Non-.unknown kinds (.text, .html) come from Ghostty keybindings that export a local
        // file (such as screen contents) and open it directly, bypassing ClickedLink checks.
        guard kind == .unknown else {
            return opensLink(URL(string: url).flatMap { $0.scheme == nil ? nil : $0 } ?? URL(filePath: url))
        }
        let link = ClickedLink(url, otherComputer: otherComputer)
        switch link.decision {
        case let .open(target):
            opensLink(target)
        case .ask, .refuse:
            linkReviewRequested?(link)
        }
    }

    func terminalDidPastePicture(_ picture: Data) {
        picturePasted?(picture)
    }

    func terminalDidRequestClipboardRead(_ answer: @escaping @MainActor (Bool) -> Void) {
        guard asksBeforeClipboardRead else { return answer(true) }
        clipboardReadRequested?(answer)
    }

    private func forward(_ callback: @escaping @MainActor (TerminalViewState) -> Void) {
        let invoke: @MainActor () -> Void = { [weak target] in
            guard let target else { return }
            callback(target)
        }
        if isDeferring {
            deferredCallbacks.append(invoke)
        } else {
            invoke()
        }
    }
}
