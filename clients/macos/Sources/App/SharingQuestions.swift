import AppKit
import KodosiBridge

/// Asks the runtime's sharing questions one at a time in an alert on the frontmost Kodosi window,
/// or app-modal while no window is open and Kodosi is in front. In the background it bounces the
/// Dock icon and never brings a window forward, so it does not take the focus from another app.
final class SharingQuestions {
    private struct Shown {
        let approval: SharingApproval
        let alert: NSAlert
        let window: NSWindow?
    }

    private let approvals: SharingApprovals
    private var shown: Shown?
    /// The sheet of the question just answered, which can still be on its way out. The next
    /// question waits behind it on the same window.
    private weak var leaving: NSWindow?
    private var attention: (approvalId: String, request: Int)?
    private var watching: [Task<Void, Never>] = []

    init(approvals: SharingApprovals) {
        self.approvals = approvals
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
        let alert = Self.alert(approval)
        shown = Shown(approval: approval, alert: alert, window: window)
        alert.beginSheetModal(for: window) { [weak self] response in
            self?.answered(approval, response)
        }
    }

    /// The modal loop runs in a task of its own, so that the question can still close while it waits.
    private func askAppModal(_ approval: SharingApproval) {
        let alert = Self.alert(approval)
        shown = Shown(approval: approval, alert: alert, window: nil)
        Task { [weak self] in
            guard self?.shown?.alert === alert else { return }
            let response = alert.runModal()
            self?.answered(approval, response)
        }
    }

    private func answered(_ approval: SharingApproval, _ response: NSApplication.ModalResponse) {
        guard let shown, shown.approval.id == approval.id else { return }
        if shown.window != nil {
            leaving = shown.alert.window
        }
        self.shown = nil
        if response == Self.allowing(approval) {
            approvals.answer(approval, allow: true)
        } else if response != .abort {
            approvals.answer(approval, allow: false)
        }
        settle()
    }

    private func close(_ shown: Shown) {
        if let window = shown.window {
            window.endSheet(shown.alert.window, returnCode: .abort)
        } else if NSApp.modalWindow === shown.alert.window {
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

    /// A question of the kodosi command can come while the person types in a terminal, so Return
    /// cancels it rather than letting a typed line allow it.
    private static func allowing(_ approval: SharingApproval) -> NSApplication.ModalResponse {
        approval.from == .command ? .alertSecondButtonReturn : .alertFirstButtonReturn
    }

    private static func alert(_ approval: SharingApproval) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = approval.title
        alert.informativeText = approval.message
        let cancel = String(localized: "Cancel")
        if allowing(approval) == .alertFirstButtonReturn {
            alert.addButton(withTitle: approval.confirm)
            alert.addButton(withTitle: cancel).keyEquivalent = "\u{1b}"
        } else {
            alert.addButton(withTitle: cancel)
            alert.addButton(withTitle: approval.confirm)
        }
        return alert
    }

    private static var frontWindow: NSWindow? {
        let open = NSApp.orderedWindows.filter { window in
            window.isVisible && window.styleMask.contains(.titled) && !(window is NSPanel) && window.sheetParent == nil
        }
        return open.first(where: \.isKeyWindow) ?? open.first
    }
}
