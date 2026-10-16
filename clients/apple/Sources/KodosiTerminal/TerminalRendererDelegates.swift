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
    TerminalSurfaceHoverLinkDelegate
{
    weak var target: TerminalViewState?
    var viewportDidResize: (() -> Void)?

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
        forward { $0.terminalDidRequestOpenURL(url, kind: kind) }
    }

    func terminalDidUpdateHoverLink(_ url: String?) {
        forward { $0.terminalDidUpdateHoverLink(url) }
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

extension TerminalViewState: @retroactive TerminalSurfaceOpenURLDelegate {
    public func terminalDidRequestOpenURL(
        _ url: String,
        kind _: TerminalOpenURLKind
    ) {
        guard let parsed = URL(string: url), parsed.scheme != nil else { return }
        #if os(macOS)
            NSWorkspace.shared.open(parsed)
        #elseif os(iOS)
            UIApplication.shared.open(parsed)
        #endif
    }
}

extension TerminalViewState: @retroactive TerminalSurfaceHoverLinkDelegate {
    public func terminalDidUpdateHoverLink(_ url: String?) {
        #if os(macOS)
            (url == nil ? NSCursor.arrow : NSCursor.pointingHand).set()
        #endif
    }
}
