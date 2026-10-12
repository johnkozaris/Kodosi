import Foundation

public final class TerminalSubscriptionRegistry: @unchecked Sendable {
    public struct SemanticCheckpoint: Sendable {
        public let bytes: Data
        public let nextSequence: UInt64
        public let rows: UInt16
        public let cols: UInt16

        public init(bytes: Data, nextSequence: UInt64, rows: UInt16, cols: UInt16) {
            self.bytes = bytes
            self.nextSequence = nextSequence
            self.rows = rows
            self.cols = cols
        }
    }

    public typealias ConnectionResult = @Sendable (Bool, String?) -> Void
    public typealias SemanticCheckpointHandler = @Sendable (SemanticCheckpoint) -> Int32

    public struct Connection: Sendable {
        public let subscriptionId: String
        public let subscriptionGeneration: UInt64
        public let onData: @Sendable (Data) -> Void
        public let onControl: @Sendable (Data) -> Void
        public let onSemanticCheckpoint: SemanticCheckpointHandler
        public let onConnectionResult: ConnectionResult

        public init(
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
