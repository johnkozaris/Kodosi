import Foundation

public final class TerminalSubscriptionRegistry: @unchecked Sendable {
    public typealias ConnectionResult = @Sendable (Bool, String?) -> Void
    /// Receives a Ghostty snapshot and whether the host terminal is a console.
    public typealias CheckpointHandler = @Sendable (Data, Bool) -> Int32

    public struct Connection: Sendable {
        public let subscriptionId: String
        public let subscriptionGeneration: UInt64
        public let onData: @Sendable (Data) -> Void
        public let onControl: @Sendable (Data) -> Void
        public let onCheckpoint: CheckpointHandler
        public let onConnectionResult: ConnectionResult

        public init(
            subscriptionId: String,
            subscriptionGeneration: UInt64,
            onData: @escaping @Sendable (Data) -> Void,
            onControl: @escaping @Sendable (Data) -> Void,
            onCheckpoint: @escaping CheckpointHandler,
            onConnectionResult: @escaping ConnectionResult
        ) {
            self.subscriptionId = subscriptionId
            self.subscriptionGeneration = subscriptionGeneration
            self.onData = onData
            self.onControl = onControl
            self.onCheckpoint = onCheckpoint
            self.onConnectionResult = onConnectionResult
        }
    }

    public struct Removal {
        public let pendingCompletion: ConnectionResult?

        public init(pendingCompletion: ConnectionResult?) {
            self.pendingCompletion = pendingCompletion
        }
    }

    private struct Entry {
        let connection: Connection
        var completionPending = true
    }

    private let lock = NSLock()
    private var connections: [String: Entry] = [:]

    @discardableResult
    public func install(_ connection: Connection, sessionId: String) -> Bool {
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

    public func connection(
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

    public func contains(
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

    public func removeExactTakingCompletion(
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

    public func takeCompletion(
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

    public func removeAll() -> [ConnectionResult] {
        lock.withLock {
            let pending = connections.values.compactMap { entry in
                entry.completionPending ? entry.connection.onConnectionResult : nil
            }
            connections.removeAll()
            return pending
        }
    }
}
