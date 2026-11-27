import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// Asks the runtime's sharing questions one at a time in a sheet on the frontmost Kodosi window,
/// or app-modal while no window is open and Kodosi is in front. In the background it bounces the
/// Dock icon and never brings a window forward, so it does not take the focus from another app.
final class SharingQuestions {
    private struct Shown {
        let approval: SharingApproval
        let sheet: NSWindow
        let window: NSWindow?
    }

    private let approvals: SharingApprovals
    private let defaults: UserDefaults
    private let dimmed: (Bool) -> Void
    private var shown: Shown? {
        didSet { dimmed(shown?.window != nil) }
    }

    /// The sheet of the question just answered, which can still be on its way out. The next
    /// question waits behind it on the same window.
    private weak var leaving: NSWindow?
    private var attention: (approvalId: String, request: Int)?
    private var watching: [Task<Void, Never>] = []

    init(approvals: SharingApprovals, defaults: UserDefaults, dimmed: @escaping (Bool) -> Void) {
        self.approvals = approvals
        self.defaults = defaults
        self.dimmed = dimmed
    }

    func start() {
        guard watching.isEmpty else { return }
        let approvals = approvals
        watching = [
            Task { [weak self] in
                for await _ in Observations({ approvals.current?.id }) {
                    self?.settle()
                }
            },
            Task { [weak self] in
                for await _ in NotificationCenter.default.notifications(named: NSApplication.didBecomeActiveNotification) {
                    self?.settle()
                }
            },
        ]
    }

    private func settle() {
        let current = approvals.current
        if let attention, attention.approvalId != current?.id {
            NSApp.cancelUserAttentionRequest(attention.request)
            self.attention = nil
        }
        if let shown {
            if shown.approval.id != current?.id {
                close(shown)
            }
            return
        }
        guard let current else { return }
        if !NSApp.isActive, attention == nil {
            attention = (current.id, NSApp.requestUserAttention(.criticalRequest))
        }
        if let window = Self.frontWindow, window.attachedSheet == nil || window.attachedSheet === leaving {
            ask(current, on: window)
        } else if NSApp.isActive {
            askAppModal(current)
        }
    }

    private func ask(_ approval: SharingApproval, on window: NSWindow) {
        let sheet = sheet(asking: approval)
        shown = Shown(approval: approval, sheet: sheet, window: window)
        window.beginSheet(sheet) { [weak self] response in
            self?.answered(approval, response)
        }
        // AppKit puts its sheet material behind the content of a sheet; the panel draws its own surface.
        for backdrop in sheet.contentView?.superview?.subviews ?? [] where backdrop is NSVisualEffectView {
            backdrop.isHidden = true
        }
    }

    /// The modal loop runs in a task of its own, so that the question can still close while it waits.
    private func askAppModal(_ approval: SharingApproval) {
        let sheet = sheet(asking: approval)
        shown = Shown(approval: approval, sheet: sheet, window: nil)
        Task { [weak self] in
            guard self?.shown?.sheet === sheet else { return }
            sheet.center()
            let response = NSApp.runModal(for: sheet)
            sheet.orderOut(nil)
            self?.answered(approval, response)
        }
    }

    private func sheet(asking approval: SharingApproval) -> NSWindow {
        let panel = SharingQuestionPanel(approval: approval) { [weak self] allow in
            self?.end(approval, allow ? .OK : .cancel)
        }
        let host = NSHostingView(rootView: panel.padding(Self.shadowRoom)
            .kodosiTheme(ground: false)
            .defaultAppStorage(defaults))
        let sheet = QuestionWindow(contentRect: NSRect(origin: .zero, size: host.fittingSize), styleMask: .borderless,
                                   backing: .buffered, defer: false)
        sheet.contentView = host
        sheet.isOpaque = false
        sheet.backgroundColor = .clear
        sheet.hasShadow = false
        return sheet
    }

    private func end(_ approval: SharingApproval, _ response: NSApplication.ModalResponse) {
        guard let shown, shown.approval.id == approval.id else { return }
        if let window = shown.window {
            window.endSheet(shown.sheet, returnCode: response)
        } else if NSApp.modalWindow === shown.sheet {
            NSApp.stopModal(withCode: response)
        }
    }

    private func answered(_ approval: SharingApproval, _ response: NSApplication.ModalResponse) {
        guard let shown, shown.approval.id == approval.id else { return }
        if shown.window != nil {
            leaving = shown.sheet
        }
        self.shown = nil
        if response == .OK {
            approvals.answer(approval, allow: true)
        } else if response != .abort {
            approvals.answer(approval, allow: false)
        }
        settle()
    }

    private func close(_ shown: Shown) {
        if let window = shown.window {
            window.endSheet(shown.sheet, returnCode: .abort)
        } else if NSApp.modalWindow === shown.sheet {
            NSApp.stopModal(withCode: .abort)
            // The modal loop sees the stop only with its next event.
            if let wake = NSEvent.otherEvent(with: .applicationDefined, location: .zero, modifierFlags: [], timestamp: 0,
                                             windowNumber: 0, context: nil, subtype: 0, data1: 0, data2: 0)
            {
                NSApp.postEvent(wake, atStart: true)
            }
        } else {
            answered(shown.approval, .abort)
        }
    }

    /// The panel draws its own shadow, which needs room inside the see-through sheet.
    private static let shadowRoom: CGFloat = 36

    private static var frontWindow: NSWindow? {
        let open = NSApp.orderedWindows.filter { window in
            window.isVisible && window.styleMask.contains(.titled) && !(window is NSPanel) && window.sheetParent == nil
        }
        return open.first(where: \.isKeyWindow) ?? open.first
    }
}

/// A borderless window takes no keys unless it says so, and the question answers Escape.
private final class QuestionWindow: NSWindow {
    override var canBecomeKey: Bool {
        true
    }
}
