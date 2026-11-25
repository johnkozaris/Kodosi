import AppKit
import CoreGraphics

extension PresenceSource {
    static let system = PresenceSource(
        isActive: { NSApp?.isActive == true },
        secondsSinceInput: secondsSinceSystemInput,
        after: { seconds, action in
            let task = Task {
                try? await Task.sleep(for: .seconds(max(seconds, 0)))
                guard !Task.isCancelled else { return }
                action()
            }
            return { task.cancel() }
        },
        nextInput: { action in
            let watch = InputWatch(action)
            return { watch.cancel() }
        },
    )
}

private func secondsSinceSystemInput() -> TimeInterval {
    CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: CGEventType(rawValue: ~0) ?? .null)
}

/// Detects user return via event monitors, app activation, and periodic idle-time polling.
/// Without Accessibility permission, macOS omits keystrokes in other apps from global monitors,
/// so polling `secondsSinceSystemInput()` catches keyboard-only activity elsewhere.
private final class InputWatch {
    private static let input: NSEvent.EventTypeMask = [
        .mouseMoved, .leftMouseDown, .rightMouseDown, .otherMouseDown, .scrollWheel, .keyDown, .flagsChanged,
    ]
    private static let readAgain: TimeInterval = 10

    private var monitors: [Any] = []
    private var activation: any NSObjectProtocol?
    private var reading: any DispatchSourceTimer?

    init(_ action: @escaping @MainActor () -> Void) {
        let fire: @MainActor () -> Void = { [weak self] in
            guard let self, !monitors.isEmpty else { return }
            cancel()
            action()
        }
        monitors = [
            NSEvent.addGlobalMonitorForEvents(matching: Self.input) { _ in MainActor.assumeIsolated(fire) },
            NSEvent.addLocalMonitorForEvents(matching: Self.input) { event in
                MainActor.assumeIsolated(fire)
                return event
            },
        ].compactMap(\.self)
        activation = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main,
        ) { _ in MainActor.assumeIsolated(fire) }
        // Use `.strict` so macOS App Nap timer coalescing does not delay idle-time checks.
        let reading = DispatchSource.makeTimerSource(flags: .strict, queue: .main)
        reading.schedule(deadline: .now() + Self.readAgain, repeating: Self.readAgain, leeway: .seconds(1))
        reading.setEventHandler {
            MainActor.assumeIsolated {
                if secondsSinceSystemInput() < Self.readAgain {
                    fire()
                }
            }
        }
        reading.resume()
        self.reading = reading
    }

    func cancel() {
        reading?.cancel()
        reading = nil
        monitors.forEach(NSEvent.removeMonitor)
        monitors.removeAll()
        if let activation {
            NSWorkspace.shared.notificationCenter.removeObserver(activation)
            self.activation = nil
        }
    }
}

/// macOS has no public screen-lock API, so combine protected-data notifications with the
/// loginwindow distributed notifications and `CGSSessionScreenIsLocked` session key.
enum SystemPresenceSignals {
    static func observe(_ reporter: PresenceReporter) {
        let session = CGSessionCopyCurrentDictionary() as? [String: Any]
        if NSApp?.isProtectedDataAvailable == false || session?["CGSSessionScreenIsLocked"] != nil {
            reporter.left(.locked)
        }
        let workspace = NSWorkspace.shared.notificationCenter
        let system = DistributedNotificationCenter.default()
        observe(.default, .NSApplicationProtectedDataWillBecomeUnavailable, reporter) { $0.left(.locked) }
        observe(.default, .NSApplicationProtectedDataDidBecomeAvailable, reporter) { $0.returned(.locked) }
        observe(system, Notification.Name("com.apple.screenIsLocked"), reporter) { $0.left(.locked) }
        observe(system, Notification.Name("com.apple.screenIsUnlocked"), reporter) { $0.returned(.locked) }
        observe(workspace, NSWorkspace.screensDidSleepNotification, reporter) { $0.left(.screensAsleep) }
        observe(workspace, NSWorkspace.screensDidWakeNotification, reporter) { $0.returned(.screensAsleep) }
        observe(workspace, NSWorkspace.willSleepNotification, reporter) { $0.left(.asleep) }
        observe(workspace, NSWorkspace.didWakeNotification, reporter) { $0.returned(.asleep) }
        observe(workspace, NSWorkspace.sessionDidResignActiveNotification, reporter) { $0.left(.switchedOut) }
        observe(workspace, NSWorkspace.sessionDidBecomeActiveNotification, reporter) { $0.returned(.switchedOut) }
    }

    /// Delivers notifications synchronously on the main queue rather than through an async task
    /// so `away` reaches the runtime before the Mac completes sleep.
    private static func observe(
        _ center: NotificationCenter, _ name: Notification.Name, _ reporter: PresenceReporter,
        _ change: @escaping @MainActor (PresenceReporter) -> Void,
    ) {
        _ = center.addObserver(forName: name, object: nil, queue: .main) { [weak reporter] _ in
            MainActor.assumeIsolated {
                if let reporter {
                    change(reporter)
                }
            }
        }
    }
}
