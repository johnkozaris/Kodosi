import AppKit

/// Buffers keystrokes typed while a newly opened terminal is attaching, then replays them to the
/// new terminal instead of letting them land in the previously focused one.
final class TerminalTypeAhead {
    private static let patience = Duration.seconds(2)
    private static let limit = 256

    private var monitor: Any?
    private var keys: [NSEvent] = []
    private var terminal: String?
    private var expiry: Task<Void, Never>?

    var isHolding: Bool {
        monitor != nil
    }

    func hold() {
        expiry?.cancel()
        expiry = Task { [weak self] in
            try? await Task.sleep(for: Self.patience)
            guard !Task.isCancelled else { return }
            self?.drop()
        }
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, !event.modifierFlags.contains(.command) else { return event }
            if keys.count < Self.limit {
                keys.append(event)
            }
            return nil
        }
    }

    func expect(_ id: String?) {
        terminal = id
    }

    /// Replays keys on the next run-loop turn after AppKit finishes updating `firstResponder`.
    func deliver(to id: String) {
        guard isHolding, id == terminal else { return }
        let held = keys
        drop()
        DispatchQueue.main.async {
            held.forEach(NSApp.sendEvent)
        }
    }

    func drop() {
        expiry?.cancel()
        expiry = nil
        if let monitor {
            NSEvent.removeMonitor(monitor)
        }
        monitor = nil
        keys = []
        terminal = nil
    }
}
