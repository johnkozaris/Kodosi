import KodosiBridge
import SwiftUI
import UIKit

/// Most questions come while a sheet is open, and an alert of the root view would close that
/// sheet, so the questions get a window of their own above the sheets and below the lock.
struct SharingQuestionWindow: UIViewRepresentable {
    let approvals: SharingApprovals
    /// The question to ask now, so that SwiftUI updates the window when it changes.
    let asking: String?
    let locked: Bool

    func makeUIView(context _: Context) -> SharingQuestionAnchor {
        SharingQuestionAnchor(approvals: approvals)
    }

    func updateUIView(_ anchor: SharingQuestionAnchor, context _: Context) {
        anchor.locked = locked
        anchor.settle()
    }

    static func dismantleUIView(_ anchor: SharingQuestionAnchor, coordinator _: ()) {
        anchor.close()
    }
}

final class SharingQuestionAnchor: UIView {
    private let approvals: SharingApprovals
    private var host: UIWindow?
    private var shown: (approval: SharingApproval, alert: UIAlertController)?
    /// UIKit takes no new alert while one comes or goes.
    private var moving = false
    private var moves = 0
    var locked = false

    init(approvals: SharingApprovals) {
        self.approvals = approvals
        super.init(frame: .zero)
        isUserInteractionEnabled = false
        accessibilityElementsHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        nil
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        settle()
    }

    func settle() {
        guard !moving else { return }
        let current = approvals.current
        if let shown, shown.approval.id != current?.id {
            self.shown = nil
            move { done in shown.alert.dismiss(animated: true) { done() } }
            return
        }
        if shown == nil {
            guard let current else {
                if host?.rootViewController?.presentedViewController == nil {
                    hide()
                } else {
                    waitForLeaving()
                }
                return
            }
            ask(current)
        }
        if shown != nil, !locked, let host, !host.isKeyWindow {
            host.makeKey()
        }
    }

    func close() {
        shown?.alert.dismiss(animated: false)
        shown = nil
        hide()
    }

    private func ask(_ approval: SharingApproval) {
        guard let scene = window?.windowScene else { return }
        let host = host ?? Self.window(in: scene)
        self.host = host
        host.isHidden = false
        guard let root = host.rootViewController else { return }
        if root.presentedViewController != nil {
            waitForLeaving()
            return
        }
        let alert = UIAlertController(title: approval.title, message: approval.message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: String(localized: "Cancel"), style: .cancel) { [weak self] _ in
            self?.answer(approval, allow: false)
        })
        let confirm = UIAlertAction(title: approval.confirm, style: .default) { [weak self] _ in
            self?.answer(approval, allow: true)
        }
        alert.addAction(confirm)
        alert.preferredAction = confirm
        shown = (approval, alert)
        move { done in root.present(alert, animated: true) { done() } }
    }

    private func answer(_ approval: SharingApproval, allow: Bool) {
        guard shown?.approval.id == approval.id else { return }
        shown = nil
        approvals.answer(approval, allow: allow)
        settle()
    }

    /// The alert that was answered can still be on its way out.
    private func waitForLeaving() {
        move { done in
            Task {
                try? await Task.sleep(for: .milliseconds(300))
                done()
            }
        }
    }

    /// UIKit drops the completion of a dismissal that it already does itself, as when a question
    /// ends as it is answered, so a move also ends after a second.
    private func move(_ transition: (@escaping @MainActor () -> Void) -> Void) {
        moves += 1
        let move = moves
        moving = true
        let done = { @MainActor [weak self] in
            guard let self, moving, moves == move else { return }
            moving = false
            settle()
        }
        transition(done)
        Task {
            try? await Task.sleep(for: .seconds(1))
            done()
        }
    }

    private func hide() {
        guard let host else { return }
        self.host = nil
        let key = host.isKeyWindow
        host.isHidden = true
        if key {
            window?.makeKey()
        }
    }

    private static func window(in scene: UIWindowScene) -> UIWindow {
        let made = UIWindow(windowScene: scene)
        made.windowLevel = .alert
        let root = UIViewController()
        root.view.backgroundColor = .clear
        made.rootViewController = root
        return made
    }
}
