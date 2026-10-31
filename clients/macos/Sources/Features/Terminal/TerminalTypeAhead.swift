import AppKit

/// Keeps the keys that a person types while a new terminal opens, so that they reach that
/// terminal and not the one before it.
@MainActor
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

    /// The new terminal took the keyboard. A responder changes after it accepts, so the keys go next.
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
