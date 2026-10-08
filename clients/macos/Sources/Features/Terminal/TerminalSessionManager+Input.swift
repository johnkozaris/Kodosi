import Foundation
import KodosiKit

extension TerminalSessionManager {
    nonisolated static func handleTerminalWrite(
        _ data: Data,
        context: TerminalWriteContext
    ) {
        guard context.token.canSendInput,
              !context.runtimeIncarnationId.isEmpty
        else { return }
        guard let generation = context.runtimeHandle.currentGeneration() else {
            context.onFailure(.runtimeUnavailable)
            return
        }
        TerminalInputQueue.shared.enqueue(
            data,
            sessionId: context.sessionId,
            coalescingToken: context.token.subscriptionGeneration,
            send: { [token = context.token] bytes in
                guard token.canSendInput else { return Int32(KODOSI_FFI_OK) }
                return context.withPointers(for: bytes) { session, incarnation, subscription, base in
                    context.runtimeHandle.withHandle(matching: generation) { handle in
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
            onFailure: context.onFailure
        )
    }
}

private extension TerminalWriteContext {
    func withPointers<T>(
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
