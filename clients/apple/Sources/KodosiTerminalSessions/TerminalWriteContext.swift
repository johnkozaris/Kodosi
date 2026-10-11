import Foundation
import KodosiBridge
import KodosiKit

struct TerminalWriteContext: Sendable {
    let sessionId: String
    let runtimeHandle: RuntimeHandle
    let token: TerminalConnectionToken
    let runtimeIncarnationId: String
    let onFailure: TerminalInputQueue.FailureHandler

    func send(_ data: Data) {
        guard token.canSendInput,
              !runtimeIncarnationId.isEmpty
        else { return }
        guard let generation = runtimeHandle.currentGeneration() else {
            onFailure(.runtimeUnavailable)
            return
        }
        TerminalInputQueue.shared.enqueue(
            data,
            sessionId: sessionId,
            coalescingToken: token.subscriptionGeneration,
            send: { [token] bytes in
                guard token.canSendInput else { return Int32(KODOSI_FFI_OK) }
                return withPointers(for: bytes) { session, incarnation, subscription, base in
                    runtimeHandle.withHandle(matching: generation) { handle in
                        kodosi_terminal_input(
                            handle,
                            session,
                            incarnation,
                            subscription,
                            token.subscriptionGeneration,
                            base,
                            UInt(bytes.count)
                        )
                    }
                }
            },
            onFailure: onFailure
        )
    }

    private func withPointers<T>(
        for bytes: Data,
        _ body: (
            UnsafePointer<CChar>,
            UnsafePointer<CChar>,
            UnsafePointer<CChar>,
            UnsafePointer<UInt8>?
        ) -> T
    ) -> T {
        sessionId.withCString { session in
            runtimeIncarnationId.withCString { incarnation in
                token.subscriptionId.withCString { subscription in
                    bytes.withUnsafeBytes { buffer in
                        body(
                            session,
                            incarnation,
                            subscription,
                            buffer.baseAddress?.assumingMemoryBound(to: UInt8.self)
                        )
                    }
                }
            }
        }
    }
}
