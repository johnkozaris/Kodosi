import Foundation
import KodosiKit
import Synchronization

@MainActor
public final class CommandSink {
    public typealias DispatchCompletion = @MainActor (Int32?) -> Void
    /// Must be thread-safe because browser pipes send commands from their background reader task.
    public typealias Transport = @Sendable (Data) -> Int32?

    public enum Delivery: Equatable, Sendable {
        case caller
        case app
    }

    private struct Pending {
        let operation: String
        let continuation: CheckedContinuation<RuntimeEvent, any Error>
    }

    private struct Abandoned: Equatable {
        let requestId: String
        let operation: String
    }

    private struct Line {
        var transport: Transport?
        var accountUserId: String?
        var accountEpoch: UInt64 = 0
    }

    private static let mostAbandoned = 64

    private nonisolated let line = Mutex(Line())
    private var pending: [String: Pending] = [:]
    /// Cancelled or timed-out requests whose late replies should be swallowed instead of routed
    /// to app-level error handling.
    private var abandoned: [Abandoned] = []
    public var onFailure: ((String) -> Void)?

    public init(transport: Transport? = nil) {
        line.withLock { $0.transport = transport }
    }

    public var transport: Transport? {
        get { line.withLock { $0.transport } }
        set { line.withLock { $0.transport = newValue } }
    }

    public func setAccount(userId: String?, epoch: UInt64) {
        let changed = line.withLock { epoch != $0.accountEpoch || userId != $0.accountUserId }
        guard changed else { return }
        cancelPending(RuntimeError.accountChanged)
        line.withLock {
            $0.accountUserId = userId
            $0.accountEpoch = epoch
        }
    }

    @discardableResult
    public func send(_ type: String, _ fields: [String: JSONValue] = [:]) -> Int32? {
        send(Self.command(type, fields))
    }

    @discardableResult
    public func send(_ command: some Encodable) -> Int32? {
        do {
            return try post(command)
        } catch {
            onFailure?(error.localizedDescription)
            return Int32(KODOSI_FFI_DESER_FAILED)
        }
    }

    nonisolated func post(_ type: String, _ fields: [String: JSONValue]) -> Int32? {
        do {
            return try post(Self.command(type, fields))
        } catch {
            return Int32(KODOSI_FFI_DESER_FAILED)
        }
    }

    private nonisolated func post(_ command: some Encodable) throws -> Int32? {
        let line = line.withLock { $0 }
        guard let transport = line.transport else { return nil }
        return try transport(JSONEncoder().encode(RuntimeCommandEnvelope(
            accountUserId: line.accountUserId,
            accountEpoch: line.accountEpoch,
            command: command,
        )))
    }

    private nonisolated static func command(_ type: String, _ fields: [String: JSONValue]) -> RuntimeCommand {
        var fields = fields
        if fields["requestId"] == nil {
            fields["requestId"] = .string(UUIDv7.generate())
        }
        return RuntimeCommand(type: type, fields: fields)
    }

    public func perform(_ type: String, _ fields: [String: JSONValue] = [:]) {
        let result = send(type, fields)
        if !Self.didAcceptSynchronously(result) {
            onFailure?(result.map { RuntimeError.rejected($0).localizedDescription }
                ?? RuntimeError.unavailable.localizedDescription)
        }
    }

    public func request(_ type: String, _ fields: [String: JSONValue] = [:]) async throws -> RuntimeEvent {
        guard pending.count < 64 else { throw RuntimeError.rejected(Int32(KODOSI_FFI_BUSY)) }
        let requestId = fields["requestId"]?.stringValue ?? UUIDv7.generate()
        guard pending[requestId] == nil else {
            throw RuntimeError.invalidResponse(String(localized: "That is already in progress.", bundle: .module))
        }
        var fields = fields
        fields["requestId"] = .string(requestId)
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume(throwing: CancellationError())
                    return
                }
                pending[requestId] = Pending(operation: type, continuation: continuation)
                let result = send(type, fields)
                guard Self.didAcceptSynchronously(result) else {
                    finish(requestId, result: .failure(result.map(RuntimeError.rejected) ?? RuntimeError.unavailable))
                    return
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in self?.abandon(requestId) }
        }
    }

    public func mutate(
        _ operation: String, session target: RuntimeSession, current: RuntimeSession?, fields: [String: JSONValue] = [:],
    ) async throws {
        guard let current, current.incarnationId == target.incarnationId else {
            throw RuntimeError.operation(String(localized: "This terminal changed. Try again.", bundle: .module))
        }
        var fields = fields
        fields["sessionId"] = .string(current.id)
        fields["expectedRuntimeIncarnationId"] = .string(current.incarnationId)
        _ = try await request(operation, fields)
    }

    @discardableResult
    public func receive(_ event: RuntimeEvent) -> Delivery {
        guard let requestId = event.string("requestId"), let operation = event.string("operation") else { return .app }
        let area = operation.prefix(while: { $0 != "." })
        let failed = event.type == area + ".error"
        guard failed || event.type == area + ".result" else { return .app }
        if let pending = pending[requestId], pending.operation == operation {
            finish(requestId, result: failed
                ? .failure(RuntimeError.operation(event.string("message") ?? String(localized: "That did not work.", bundle: .module)))
                : .success(event))
            return .caller
        }
        if let index = abandoned.firstIndex(of: Abandoned(requestId: requestId, operation: operation)) {
            abandoned.remove(at: index)
            return .caller
        }
        return .app
    }

    public func cancelPending(_ error: any Error = RuntimeError.unavailable) {
        let callbacks = Array(pending.values)
        pending.removeAll()
        for callback in callbacks {
            callback.continuation.resume(throwing: error)
        }
    }

    /// Fails all in-flight requests after dropped runtime events; late replies are ignored via
    /// `abandoned`.
    public func abandonPending() {
        let waiting = pending
        pending.removeAll()
        for (requestId, request) in waiting {
            remember(Abandoned(requestId: requestId, operation: request.operation))
            request.continuation.resume(throwing: RuntimeError.timedOut)
        }
    }

    private func finish(_ requestId: String, result: Result<RuntimeEvent, any Error>) {
        pending.removeValue(forKey: requestId)?.continuation.resume(with: result)
    }

    private func abandon(_ requestId: String) {
        guard let pending = pending.removeValue(forKey: requestId) else { return }
        remember(Abandoned(requestId: requestId, operation: pending.operation))
        pending.continuation.resume(throwing: CancellationError())
    }

    private func remember(_ request: Abandoned) {
        abandoned.append(request)
        if abandoned.count > Self.mostAbandoned {
            abandoned.removeFirst()
        }
    }

    public static func didAcceptSynchronously(_ result: Int32?) -> Bool {
        result == Int32(KODOSI_FFI_OK)
    }

    /// Pass `returning: true` when focus is restored by app activation or reconnect rather than
    /// user interaction, so the host does not resize the terminal to this client.
    public func sendTerminalFocus(sessionId: String, clientId: String, subscriptionGeneration: UInt64, requestId: String,
                                  expectedRuntimeIncarnationId: String, returning: Bool = false) -> Int32?
    {
        send(TerminalCommand.focus(sessionId: sessionId, clientId: clientId, subscriptionGeneration: subscriptionGeneration,
                                   requestId: requestId, expectedRuntimeIncarnationId: expectedRuntimeIncarnationId,
                                   returning: returning))
    }

    public func sendTerminalBlur(sessionId: String, clientId: String, subscriptionGeneration: UInt64,
                                 expectedRuntimeIncarnationId: String) -> Int32?
    {
        send(TerminalCommand.blur(sessionId: sessionId, clientId: clientId, subscriptionGeneration: subscriptionGeneration,
                                  requestId: UUIDv7.generate(), expectedRuntimeIncarnationId: expectedRuntimeIncarnationId))
    }

    @discardableResult
    public func sendTerminalResize(sessionId: String, identity: TerminalResizeIdentity,
                                   completion: DispatchCompletion? = nil) -> Int32?
    {
        let result = send(TerminalCommand.resize(sessionId: sessionId, identity: identity))
        completion?(result)
        return result
    }
}
