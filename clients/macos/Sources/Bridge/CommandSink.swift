import Foundation
import KodosiKit

@MainActor
final class CommandSink {
    typealias DispatchCompletion = @MainActor (Int32?) -> Void
    typealias Transport = @MainActor (Data) -> Int32?

    private struct Pending {
        let operation: String
        let replyType: String
        let continuation: CheckedContinuation<RuntimeEvent, any Error>
    }

    var transport: Transport?
    private(set) var accountUserId: String?
    private(set) var accountEpoch: UInt64 = 0
    private var pending: [String: Pending] = [:]
    private var timeouts: [String: Task<Void, Never>] = [:]
    var onFailure: ((String) -> Void)?

    init(transport: Transport? = nil) {
        self.transport = transport
    }

    func setAccount(userId: String?, epoch: UInt64) {
        guard epoch != accountEpoch || userId != accountUserId else { return }
        cancelPending(RuntimeError.accountChanged)
        accountUserId = userId
        accountEpoch = epoch
    }

    @discardableResult
    func send(_ type: String, _ fields: [String: JSONValue] = [:]) -> Int32? {
        send(RuntimeCommand(type: type, fields: fields))
    }

    @discardableResult
    func send(_ command: some Encodable) -> Int32? {
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

    func perform(_ type: String, _ fields: [String: JSONValue] = [:]) {
        let result = send(type, fields)
        if !Self.didAcceptSynchronously(result) {
            onFailure?(result.map { RuntimeError.rejected($0).localizedDescription }
                ?? RuntimeError.unavailable.localizedDescription)
        }
    }

    func request(_ type: String, _ fields: [String: JSONValue] = [:]) async throws -> RuntimeEvent {
        guard pending.count < 64 else { throw RuntimeError.rejected(Int32(KODOSI_FFI_BUSY)) }
        let requestId = fields["requestId"]?.stringValue ?? UUIDv7.generate()
        guard pending[requestId] == nil else {
            throw RuntimeError.invalidResponse(String(localized: "An action with this identifier is already pending."))
        }
        var fields = fields
        fields["requestId"] = .string(requestId)
        let replyType = if type == "mission.open" {
            "mission.snapshot"
        } else if type.hasPrefix("provider.") {
            "provider.reply"
        } else {
            String(type.prefix(while: { $0 != "." })) + ".result"
        }
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume(throwing: CancellationError())
                    return
                }
                pending[requestId] = Pending(operation: type, replyType: replyType, continuation: continuation)
                let result = send(type, fields)
                guard Self.didAcceptSynchronously(result) else {
                    finish(requestId, result: .failure(result.map(RuntimeError.rejected) ?? RuntimeError.unavailable))
                    return
                }
                guard pending[requestId] != nil else { return }
                timeouts[requestId] = Task { @MainActor [weak self] in
                    try? await Task.sleep(for: .seconds(type == "room.command" ? 60 : 20))
                    guard !Task.isCancelled else { return }
                    self?.finish(requestId, result: .failure(RuntimeError.timedOut))
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in self?.finish(requestId, result: .failure(CancellationError())) }
        }
    }

    func receive(_ event: RuntimeEvent) {
        guard event.accountEpoch == accountEpoch, event.accountUserId == accountUserId,
              let requestId = event.string("requestId"), let pending = pending[requestId] else { return }
        let errorType = String(pending.operation.prefix(while: { $0 != "." })) + ".error"
        let isError = event.type == errorType
        guard event.type == pending.replyType || isError,
              event.type == "mission.snapshot" || event.string("operation") == pending.operation else { return }
        if isError {
            finish(requestId, result: .failure(RuntimeError.operation(event.string("message") ?? String(localized: "The action failed."))))
        } else {
            finish(requestId, result: .success(event))
        }
    }

    func cancelPending(_ error: any Error = RuntimeError.unavailable) {
        let callbacks = Array(pending.values)
        pending.removeAll()
        timeouts.values.forEach { $0.cancel() }
        timeouts.removeAll()
        for callback in callbacks {
            callback.continuation.resume(throwing: error)
        }
    }

    private func finish(_ requestId: String, result: Result<RuntimeEvent, any Error>) {
        timeouts.removeValue(forKey: requestId)?.cancel()
        pending.removeValue(forKey: requestId)?.continuation.resume(with: result)
    }

    static func didAcceptSynchronously(_ result: Int32?) -> Bool {
        result == Int32(KODOSI_FFI_OK)
    }

    func setHostTheme(dark: Bool) {
        perform("system.setTheme", ["dark": .bool(dark)])
    }

    func sendTerminalFocus(sessionId: String, clientId: String, subscriptionGeneration: UInt64, requestId: String,
                           expectedRuntimeIncarnationId: String) -> Int32?
    {
        send(TerminalCommand.focus(sessionId: sessionId, clientId: clientId, subscriptionGeneration: subscriptionGeneration,
                                   requestId: requestId, expectedRuntimeIncarnationId: expectedRuntimeIncarnationId))
    }

    func sendTerminalBlur(sessionId: String, clientId: String, subscriptionGeneration: UInt64,
                          expectedRuntimeIncarnationId: String) -> Int32?
    {
        send(TerminalCommand.blur(sessionId: sessionId, clientId: clientId, subscriptionGeneration: subscriptionGeneration,
                                  expectedRuntimeIncarnationId: expectedRuntimeIncarnationId))
    }

    @discardableResult
    func sendTerminalResize(sessionId: String, identity: TerminalResizeIdentity,
                            claim: Bool = false, completion: DispatchCompletion? = nil) -> Int32?
    {
        let result = send(TerminalCommand.resize(sessionId: sessionId, identity: identity, claim: claim))
        completion?(result)
        return result
    }
}
