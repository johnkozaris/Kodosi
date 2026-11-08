import KodosiDesign
import LocalAuthentication
import SwiftUI

/// Kodosi's lock on this device. When the person turns it on, Kodosi asks for Face ID, Touch ID
/// or the passcode when it opens and after a minute away, and covers what it shows while it is
/// not in front, also in the app switcher.
@Observable
final class AppLock {
    static let storageKey = "privacy.lock"
    nonisolated static let away: TimeInterval = 60

    /// How this device proves the person: the name of its biometry, or nil when it has no
    /// passcode, so nothing can lock.
    enum Proof {
        case faceID
        case touchID
        case opticID
        case passcode

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

    private(set) var locked: Bool
    private(set) var enabled: Bool
    @ObservationIgnored private var left: Date?
    @ObservationIgnored private var asking = false
    @ObservationIgnored private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let enabled = defaults.bool(forKey: Self.storageKey)
        self.enabled = enabled
        locked = enabled
    }

    /// Whether Kodosi locks again when it comes back at `now` after it left at `left`.
    nonisolated static func locks(enabled: Bool, left: Date?, now: Date) -> Bool {
        guard enabled, let left else { return false }
        return now.timeIntervalSince(left) >= away
    }

    func covers(_ phase: ScenePhase) -> Bool {
        locked || (enabled && phase != .active)
    }

    func sceneChanged(to phase: ScenePhase, now: Date = .now) {
        switch phase {
        case .background:
            left = left ?? now
        case .active:
            if Self.locks(enabled: enabled, left: left, now: now) {
                locked = true
            }
            left = nil
            if locked {
                unlock()
            }
        default:
            break
        }
    }

    func unlock() {
        guard locked, !asking else { return }
        guard Proof.current != nil else {
            locked = false
            return
        }
        asking = true
        Task {
            let proved = await prove()
            asking = false
            if proved {
                locked = false
            }
        }
    }

    /// Turns the lock on after the person proves it is them, so the lock works when it is on.
    func turn(on: Bool) async {
        if on, await !prove() {
            return
        }
        enabled = on
        defaults.set(on, forKey: Self.storageKey)
    }

    private func prove() async -> Bool {
        let reason = String(localized: "Unlock Kodosi to see your terminals.")
        return await (try? LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)) ?? false
    }
}

/// What shows over Kodosi while it is locked or not in front.
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
        .accessibilityIdentifier("lock.cover")
        .onAppear { model.lock.unlock() }
    }
}
