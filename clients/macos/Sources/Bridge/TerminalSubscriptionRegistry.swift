import Foundation

final class TerminalSubscriptionRegistry: @unchecked Sendable {
    struct SemanticCheckpoint: Sendable {
        let bytes: Data
        let nextSequence: UInt64
        let rows: UInt16
        let cols: UInt16
    }

    typealias ConnectionResult = @Sendable (Bool, String?) -> Void
    typealias SemanticCheckpointHandler = @Sendable (SemanticCheckpoint) -> Int32

    struct Connection: Sendable {
        let subscriptionId: String
        let subscriptionGeneration: UInt64
        let onData: @Sendable (Data) -> Void
        let onControl: @Sendable (Data) -> Void
        let onSemanticCheckpoint: SemanticCheckpointHandler
        let onConnectionResult: ConnectionResult

        init(
            subscriptionId: String,
            subscriptionGeneration: UInt64,
            onData: @escaping @Sendable (Data) -> Void,
            onControl: @escaping @Sendable (Data) -> Void,
            onSemanticCheckpoint: @escaping SemanticCheckpointHandler,
            onConnectionResult: @escaping ConnectionResult
        ) {
            self.subscriptionId = subscriptionId
            self.subscriptionGeneration = subscriptionGeneration
            self.onData = onData
            self.onControl = onControl
            self.onSemanticCheckpoint = onSemanticCheckpoint
            self.onConnectionResult = onConnectionResult
        }
    }

    struct Removal {
        let pendingCompletion: ConnectionResult?
    }

    private struct Entry {
        let connection: Connection
        var completionPending = true
    }

    private let lock = NSLock()
    private var connections: [String: Entry] = [:]

    @discardableResult
    func install(_ connection: Connection, sessionId: String) -> Bool {
        lock.withLock {
            if let current = connections[sessionId]?.connection,
               current.subscriptionGeneration >= connection.subscriptionGeneration
            {
                return false
            }
            connections[sessionId] = Entry(connection: connection)
            return true
        }
    }

    func connection(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64
    ) -> Connection? {
        lock.withLock {
            guard let connection = connections[sessionId]?.connection,
                  connection.subscriptionId == subscriptionId,
                  connection.subscriptionGeneration == subscriptionGeneration
            else {
                return nil
            }
            return connection
        }
    }

    func contains(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64
    ) -> Bool {
        connection(
            sessionId: sessionId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration
        ) != nil
    }

    func removeExactTakingCompletion(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64
    ) -> Removal? {
        lock.withLock {
            guard let entry = connections[sessionId],
                  entry.connection.subscriptionId == subscriptionId,
                  entry.connection.subscriptionGeneration == subscriptionGeneration
            else {
                return nil
            }
            let connection = entry.connection
            connections.removeValue(forKey: sessionId)
            return Removal(
                pendingCompletion: entry.completionPending
                    ? connection.onConnectionResult
                    : nil
            )
        }
    }

    func takeCompletion(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64,
        keepConnection: Bool
    ) -> ConnectionResult? {
        lock.withLock {
            guard var entry = connections[sessionId],
                  entry.connection.subscriptionId == subscriptionId,
                  entry.connection.subscriptionGeneration == subscriptionGeneration,
                  entry.completionPending
            else {
                return nil
            }
            entry.completionPending = false
            let completion = entry.connection.onConnectionResult
            if keepConnection {
                connections[sessionId] = entry
            } else {
                connections.removeValue(forKey: sessionId)
            }
            return completion
        }
    }

    func removeAll() -> [ConnectionResult] {
        lock.withLock {
            let pending = connections.values.compactMap { entry in
                entry.completionPending ? entry.connection.onConnectionResult : nil
            }
            connections.removeAll()
            return pending
        }
    }
}
