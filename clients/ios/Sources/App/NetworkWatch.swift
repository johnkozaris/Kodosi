import Foundation
import Network

/// The runtime cannot see on iOS that the phone moved to another network, so the app tells it.
@MainActor
final class NetworkWatch {
    private let monitor = NWPathMonitor()
    private var route: String?
    private var settling: Task<Void, Never>?
    private var onChange: (@MainActor () -> Void)?

    func start(onChange: @escaping @MainActor () -> Void) {
        self.onChange = onChange
        monitor.pathUpdateHandler = { [weak self] path in
            let route = Self.route(of: path)
            Task { @MainActor in self?.update(route) }
        }
        monitor.start(queue: DispatchQueue(label: "Kodosi network"))
    }

    private func update(_ route: String) {
        let known = self.route
        self.route = route
        guard let known, known != route else { return }
        settling?.cancel()
        settling = Task { [weak self] in
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled else { return }
            self?.onChange?()
        }
    }

    private nonisolated static func route(of path: NWPath) -> String {
        ([String(describing: path.status)] + path.availableInterfaces.map(\.name)).joined(separator: " ")
    }
}
