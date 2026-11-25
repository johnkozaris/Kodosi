import KodosiDesign
import LocalAuthentication
import SwiftUI
import UIKit

@Observable
final class AppLock {
    static let storageKey = "privacy.lock"
    nonisolated static let away: Duration = .seconds(60)

    enum Proof {
        case faceID
        case touchID
        case opticID
        case passcode

        /// `nil` when the device has no passcode, so nothing can lock.
        static var current: Proof? {
            let context = LAContext()
            guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil) else { return nil }
            return switch context.biometryType {
            case .faceID: .faceID
            case .touchID: .touchID
            case .opticID: .opticID
            default: .passcode
            }
        }
    }

    enum Answer {
        case proved
        case refused
        case interrupted
    }

    private(set) var locked: Bool
    private(set) var enabled: Bool
    @ObservationIgnored private var left: ContinuousClock.Instant?
    @ObservationIgnored private var asking = false
    @ObservationIgnored private var asked = false
    @ObservationIgnored private let defaults: UserDefaults
    @ObservationIgnored private let canProve: () -> Bool
    @ObservationIgnored private let ask: @MainActor () async -> Answer

    init(
        defaults: UserDefaults = .standard,
        canProve: @escaping () -> Bool = { Proof.current != nil },
        ask: @escaping @MainActor () async -> Answer = AppLock.askOwner,
    ) {
        self.defaults = defaults
        self.canProve = canProve
        self.ask = ask
        let enabled = defaults.bool(forKey: Self.storageKey)
        self.enabled = enabled
        locked = enabled
    }

    nonisolated static func locks(enabled: Bool, left: ContinuousClock.Instant?, now: ContinuousClock.Instant) -> Bool {
        guard enabled, let left else { return false }
        return left.duration(to: now) >= away
    }

    func covers(_ phase: ScenePhase) -> Bool {
        locked || (enabled && phase != .active)
    }

    /// Prompt at most once per foreground visit. The system authentication sheet makes the scene
    /// `.inactive` and then `.active` again on dismissal, so `asked` prevents a cancel loop.
    func sceneChanged(to phase: ScenePhase, now: ContinuousClock.Instant = .now) {
        switch phase {
        case .background:
            left = left ?? now
            asked = false
        case .active:
            if Self.locks(enabled: enabled, left: left, now: now) {
                locked = true
            }
            left = nil
            if locked, !asked {
                unlock()
            }
        default:
            break
        }
    }

    func unlock() {
        guard locked, !asking else { return }
        guard canProve() else {
            locked = false
            return
        }
        asking = true
        asked = true
        Task {
            let answer = await ask()
            asking = false
            switch answer {
            case .proved: locked = false
            case .refused: break
            case .interrupted: asked = false
            }
        }
    }

    /// Asks the owner to authenticate before turning the lock on, so the lock is known to work
    /// once it is on.
    func turn(on: Bool) async {
        if on, !canProve() {
            return
        }
        if on, await ask() != .proved {
            return
        }
        enabled = on
        defaults.set(on, forKey: Self.storageKey)
    }

    static func askOwner() async -> Answer {
        let reason = String(localized: "Unlock Kodosi to see your terminals.")
        do {
            return try await LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
                ? .proved : .refused
        } catch let error as LAError where [.systemCancel, .appCancel, .notInteractive].contains(error.code) {
            return .interrupted
        } catch {
            return .refused
        }
    }
}

struct LockCover: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    var body: some View {
        ZStack {
            GroundBackdrop()
            VStack(spacing: 20) {
                KodosiWordmark(size: 30, blinks: false)
                if model.lock.locked {
                    Text("Kodosi is locked")
                        .appTextStyle(.body)
                        .foregroundStyle(theme.colors.inkMuted)
                    Button("Unlock") { model.lock.unlock() }
                        .buttonStyle(.kodosi(.primary, size: .large))
                        .accessibilityIdentifier("lock.unlock")
                }
            }
            .padding(32)
        }
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(.isModal)
        .accessibilityIdentifier("lock.cover")
        .onAppear { model.lock.unlock() }
    }
}

/// Hosts the lock cover in a dedicated `UIWindow` above sheets, alerts, and full-screen covers,
/// taking key-window focus while locked so hardware keyboard input cannot reach views underneath.
struct LockWindow<Cover: View>: UIViewRepresentable {
    let covers: Bool
    let locked: Bool
    let cover: () -> Cover

    func makeUIView(context _: Context) -> LockAnchor {
        LockAnchor()
    }

    func updateUIView(_ anchor: LockAnchor, context _: Context) {
        anchor.show(covers ? { UIHostingController(rootView: cover()) } : nil, locked: locked)
    }

    static func dismantleUIView(_ anchor: LockAnchor, coordinator _: ()) {
        anchor.show(nil, locked: false)
    }
}

final class LockAnchor: UIView {
    private var cover: (() -> UIViewController)?
    private var locked = false
    private(set) var shown: UIWindow?

    init() {
        super.init(frame: .zero)
        isUserInteractionEnabled = false
        accessibilityElementsHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        nil
    }

    func show(_ cover: (() -> UIViewController)?, locked: Bool) {
        self.cover = cover
        self.locked = locked
        update()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        update()
    }

    private func update() {
        guard let cover else {
            hide()
            return
        }
        guard let main = window, let scene = main.windowScene else { return }
        let shown = shown ?? {
            let made = UIWindow(windowScene: scene)
            made.windowLevel = .alert + 1
            made.rootViewController = cover()
            self.shown = made
            return made
        }()
        shown.isHidden = false
        if locked, !shown.isKeyWindow {
            main.endEditing(true)
            shown.makeKey()
        } else if !locked, shown.isKeyWindow {
            main.makeKey()
        }
    }

    private func hide() {
        guard let shown else { return }
        self.shown = nil
        let key = shown.isKeyWindow
        shown.isHidden = true
        if key {
            window?.makeKey()
        }
    }
}
