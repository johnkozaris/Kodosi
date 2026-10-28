#if os(macOS)
    import AppKit
#elseif os(iOS)
    import UIKit
#endif
import Foundation
import GhosttyTerminal

@MainActor
final class TerminalRendererDelegateProxy:
    TerminalSurfaceGridResizeDelegate,
    TerminalSurfaceLifecycleDelegate,
    TerminalSurfaceOpenURLDelegate,
    TerminalSurfaceHoverLinkDelegate,
    TerminalSurfaceClipboardReadDelegate
{
    weak var target: TerminalViewState?
    var viewportDidResize: (() -> Void)?
    var clipboardReadRequested: ((@escaping @MainActor (Bool) -> Void) -> Void)?
    var asksBeforeClipboardRead = true
    /// The computer that runs the terminal, when it is not this device.
    var otherComputer: String?
    var linkReviewRequested: ((TerminalLink) -> Void)?
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
        Task { @MainActor [weak self] in
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
        forward { $0.terminalDidAttachSurface(surface) }
    }

    func terminalDidDetachSurface() {
        forward { $0.terminalDidDetachSurface() }
    }

    func terminalDidRequestOpenURL(_ url: String, kind: TerminalOpenURLKind) {
        // Only a key that the person presses makes Ghostty write a file on this
        // device, such as the screen, and open it. Ghostty opens these as they are.
        guard kind == .unknown else {
            return opensLink(URL(string: url).flatMap { $0.scheme == nil ? nil : $0 } ?? URL(filePath: url))
        }
        let link = TerminalLink(url, otherComputer: otherComputer)
        if case let .open(target) = link.decision {
            opensLink(target)
        } else {
            linkReviewRequested?(link)
        }
    }

    func terminalDidUpdateHoverLink(_ url: String?) {
        forward { $0.terminalDidUpdateHoverLink(url) }
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

extension TerminalViewState: @retroactive TerminalSurfaceHoverLinkDelegate {
    public func terminalDidUpdateHoverLink(_ url: String?) {
        #if os(macOS)
            (url == nil ? NSCursor.arrow : NSCursor.pointingHand).set()
        #endif
    }
}
