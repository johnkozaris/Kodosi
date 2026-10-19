import Foundation
import KodosiKit

@MainActor
public final class CommandSink {
    public typealias DispatchCompletion = @MainActor (Int32?) -> Void
    public typealias Transport = @MainActor (Data) -> Int32?

    public enum Delivery: Equatable, Sendable {
        case caller
        case app
    }

    private struct Pending {
        let operation: String
        let continuation: CheckedContinuation<RuntimeEvent, any Error>
    }

    public var transport: Transport?
    private(set) var accountUserId: String?
    private(set) var accountEpoch: UInt64 = 0
    private var pending: [String: Pending] = [:]
    public var onFailure: ((String) -> Void)?

    public init(transport: Transport? = nil) {
        self.transport = transport
    }

    public func setAccount(userId: String?, epoch: UInt64) {
        guard epoch != accountEpoch || userId != accountUserId else { return }
        cancelPending(RuntimeError.accountChanged)
        accountUserId = userId
        accountEpoch = epoch
    }

    @discardableResult
    public func send(_ type: String, _ fields: [String: JSONValue] = [:]) -> Int32? {
        var fields = fields
        if fields["requestId"] == nil {
            fields["requestId"] = .string(UUIDv7.generate())
        }
        return send(RuntimeCommand(type: type, fields: fields))
    }

    @discardableResult
    public func send(_ command: some Encodable) -> Int32? {
        guard let transport else { return nil }
        do {
            return try transport(JSONEncoder().encode(RuntimeCommandEnvelope(
                accountUserId: accountUserId,
                accountEpoch: accountEpoch,
                command: command
            )))
        } catch {
            onFailure?(error.localizedDescription)
            return Int32(KODOSI_FFI_DESER_FAILED)
        }
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
            Task { @MainActor [weak self] in self?.finish(requestId, result: .failure(CancellationError())) }
        }
    }

    public func mutate(
        _ operation: String, session target: RuntimeSession, current: RuntimeSession?, fields: [String: JSONValue] = [:]
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
        guard let requestId = event.string("requestId"), let pending = pending[requestId],
              event.string("operation") == pending.operation else { return .app }
        let area = pending.operation.prefix(while: { $0 != "." })
        if event.type == area + ".error" {
            finish(requestId, result: .failure(RuntimeError.operation(
                event.string("message") ?? String(localized: "That did not work.", bundle: .module)
            )))
        } else if event.type == area + ".result" {
            finish(requestId, result: .success(event))
        } else {
            return .app
        }
        return .caller
    }

    public func cancelPending(_ error: any Error = RuntimeError.unavailable) {
        let callbacks = Array(pending.values)
        pending.removeAll()
        for callback in callbacks {
            callback.continuation.resume(throwing: error)
        }
    }

    private func finish(_ requestId: String, result: Result<RuntimeEvent, any Error>) {
        pending.removeValue(forKey: requestId)?.continuation.resume(with: result)
    }

    public static func didAcceptSynchronously(_ result: Int32?) -> Bool {
        result == Int32(KODOSI_FFI_OK)
    }

    public func sendTerminalFocus(sessionId: String, clientId: String, subscriptionGeneration: UInt64, requestId: String,
                                  expectedRuntimeIncarnationId: String) -> Int32?
    {
        send(TerminalCommand.focus(sessionId: sessionId, clientId: clientId, subscriptionGeneration: subscriptionGeneration,
                                   requestId: requestId, expectedRuntimeIncarnationId: expectedRuntimeIncarnationId))
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
