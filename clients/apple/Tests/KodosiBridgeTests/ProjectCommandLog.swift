import Foundation

enum ProjectTestFailure: Error {
    case missing(String)
}

final class ProjectCommandLog: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [[String: Any]] = []

    func append(_ data: Data) {
        guard let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        lock.withLock { values.append(value) }
    }

    func next(_ type: String) async throws -> [String: Any] {
        for _ in 0 ..< 1000 {
            if let found = take(type) {
                return found
            }
            try await Task.sleep(for: .milliseconds(2))
        }
        throw ProjectTestFailure.missing(type)
    }

    func take(_ type: String) -> [String: Any]? {
        lock.withLock {
            guard let index = values.firstIndex(where: { $0["type"] as? String == type }) else { return nil }
            return values.remove(at: index)
        }
    }
}
