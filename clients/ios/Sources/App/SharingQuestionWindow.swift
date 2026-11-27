import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

/// Most questions come while a sheet is open, so the questions get a window of their own above the
/// sheets and below the lock.
struct SharingQuestionWindow: UIViewRepresentable {
    let approvals: SharingApprovals
    let lock: AppLock
    /// The question to ask now and the lock, so that SwiftUI updates the window when they change.
    let asking: String?
    let locked: Bool

    func makeUIView(context _: Context) -> SharingQuestionAnchor {
        SharingQuestionAnchor(approvals: approvals, lock: lock)
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
    private let lock: AppLock
    private var host: UIWindow?
    /// The window stays while the last question leaves.
    private var hiding: Task<Void, Never>?
    var locked = false

    init(approvals: SharingApprovals, lock: AppLock) {
        self.approvals = approvals
        self.lock = lock
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
        guard approvals.current != nil else {
            if let host, hiding == nil {
                // The window stays for the fade, but taps and keys go back to the app at once.
                host.isUserInteractionEnabled = false
                if host.isKeyWindow {
                    window?.makeKey()
                }
                hiding = Task { [weak self] in
                    try? await Task.sleep(for: .milliseconds(400))
                    guard !Task.isCancelled else { return }
                    self?.close()
                }
            }
            return
        }
        hiding?.cancel()
        hiding = nil
        guard let scene = window?.windowScene else { return }
        let host = host ?? Self.window(in: scene, approvals: approvals, lock: lock)
        self.host = host
        host.isUserInteractionEnabled = true
        host.isHidden = false
        if !locked, !host.isKeyWindow {
            window?.endEditing(true)
            host.makeKey()
        }
    }

    func close() {
        hiding?.cancel()
        hiding = nil
        guard let host else { return }
        self.host = nil
        let key = host.isKeyWindow
        host.isHidden = true
        if key {
            window?.makeKey()
        }
    }

    private static func window(in scene: UIWindowScene, approvals: SharingApprovals, lock: AppLock) -> UIWindow {
        let made = UIWindow(windowScene: scene)
        made.windowLevel = .alert
        let root = UIHostingController(rootView: SharingQuestionLayer(approvals: approvals, lock: lock).kodosiTheme(ground: false))
        root.view.backgroundColor = .clear
        made.rootViewController = root
        return made
    }
}

private struct SharingQuestionLayer: View {
    @Environment(\.theme) private var theme
    let approvals: SharingApprovals
    let lock: AppLock

    var body: some View {
        ZStack {
            if let approval = approvals.current {
                theme.colors.shadow.opacity(theme.isDark ? 0.55 : 0.22)
                    .ignoresSafeArea()
                    .accessibilityHidden(true)
                    .transition(.opacity)
                GeometryReader { screen in
                    ScrollView {
                        SharingQuestionPanel(approval: approval, covered: lock.locked) { allow in
                            approvals.answer(approval, allow: allow)
                        }
                        .padding(16)
                        .frame(maxWidth: .infinity, minHeight: screen.size.height)
                    }
                    .scrollBounceBehavior(.basedOnSize)
                }
                .id(approval.id)
                .transition(.asymmetric(
                    insertion: .scale(scale: 0.95).combined(with: .offset(y: 18)).combined(with: .opacity),
                    removal: .opacity,
                ))
            }
        }
        .animation(theme.motion.spring, value: approvals.current?.id)
    }
}
