import Foundation
import KodosiKit
import os

final class RuntimeHandle: @unchecked Sendable {
    private let handleSlot = RuntimeHandleSlot()
    private var retainedSelf: Unmanaged<RuntimeHandle>?
    @MainActor private var startGeneration: UInt64 = 0
    @MainActor private var starting = false

    private let terminalSubscriptions = TerminalSubscriptionRegistry()

    private let inbox: RuntimeEventInbox

    init(inbox: RuntimeEventInbox) {
        self.inbox = inbox
    }

    var isRunning: Bool {
        handleSlot.isRunning
    }

    func currentGeneration() -> UInt64? {
        handleSlot.currentGeneration()
    }

    @discardableResult
    func withHandle<T>(
        matching expectedGeneration: UInt64,
        _ body: (UnsafeMutableRawPointer) -> T
    ) -> T? {
        guard let snapshot = handleSlot.snapshot(matching: expectedGeneration) else {
            return nil
        }
        return body(snapshot.pointer)
    }

    static let expectedFFIABIVersion: Int32 = 7

    static let minimumSupportedProtocolVersion: UInt32 = 51
    static let maximumSupportedProtocolVersion: UInt32 = 51

    static func isSupportedProtocolVersion(_ version: UInt32) -> Bool {
        (minimumSupportedProtocolVersion ... maximumSupportedProtocolVersion).contains(version)
    }

    enum StartOutcome: Equatable {
        case started
        case incompatibleRuntime
        case failed(RuntimeStartFailure?)
    }

    @MainActor
    func start(commandSink: CommandSink, completion: @escaping @MainActor (StartOutcome) -> Void) {
        guard Int32(KODOSI_FFI_ABI_VERSION) == Self.expectedFFIABIVersion,
              kodosi_abi_version() == UInt32(Self.expectedFFIABIVersion),
              Self.isSupportedProtocolVersion(kodosi_protocol_version())
        else {
            Logger.runtime.error("The embedded runtime does not match this app's ABI and protocol.")
            completion(.incompatibleRuntime)
            return
        }
        guard !starting else { completion(.failed(nil)); return }
        if isRunning {
            completion(.started); return
        }
        starting = true
        startGeneration &+= 1
        let generation = startGeneration
        DispatchQueue.global(qos: .userInitiated).async { [self] in
            let retained = Unmanaged.passRetained(self)
            var callbacks = kodosi_callbacks_t(
                on_event: Self.onEvent,
                on_terminal_data: Self.onTerminalData,
                on_terminal_control: Self.onTerminalControl,
                on_terminal_connect_result: Self.onTerminalConnectResult,
                on_terminal_checkpoint: Self.onTerminalCheckpoint
            )
            let handle = withUnsafePointer(to: &callbacks) {
                kodosi_start($0, UInt(MemoryLayout<kodosi_callbacks_t>.size), retained.toOpaque())
            }
            let address = handle.map { UInt(bitPattern: $0) }
            let failure = handle == nil ? RuntimeStartFailure.last() : nil
            Task { @MainActor [self] in
                starting = false
                guard generation == startGeneration else {
                    DispatchQueue.global(qos: .userInitiated).async {
                        if let address {
                            kodosi_stop(UnsafeMutableRawPointer(bitPattern: address))
                        }
                        retained.release()
                    }
                    return
                }
                guard let address, let handle = UnsafeMutableRawPointer(bitPattern: address) else {
                    retained.release()
                    completion(.failed(failure))
                    return
                }
                retainedSelf = retained
                handleSlot.install(handle)
                commandSink.transport = { [weak self] data in
                    guard let self, let current = currentGeneration() else { return nil }
                    return withHandle(matching: current) { handle in
                        data.withUnsafeBytes { bytes in
                            kodosi_send_command(handle, bytes.bindMemory(to: UInt8.self).baseAddress, UInt(data.count))
                        }
                    }
                }
                completion(.started)
            }
        }
    }

    @MainActor
    func stop(completion: @escaping @MainActor () -> Void = {}) {
        startGeneration &+= 1
        let snapshot = handleSlot.tombstone()
        let pendingTerminalConnections = terminalSubscriptions.removeAll()
        for pending in pendingTerminalConnections {
            pending(false, String(localized: "Kodosi stopped before the terminal connected."))
        }

        guard let snapshot else {
            completion()
            return
        }
        let retained = retainedSelf
        retainedSelf = nil

        Logger.runtime.info("Stopping Rust runtime...")
        DispatchQueue.global(qos: .userInitiated).async {
            kodosi_stop(snapshot.pointer)
            Task { @MainActor in
                Logger.runtime.info("Rust runtime stopped")
                retained?.release()
                completion()
            }
        }
    }

    func refreshTerminal(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64
    ) {
        guard terminalSubscriptions.contains(
            sessionId: sessionId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration
        ) else {
            Logger.terminal.debug(
                "Ignored stale terminal refresh for \(sessionId), subscription \(subscriptionId)"
            )
            return
        }
        guard let generation = handleSlot.currentGeneration() else { return }
        let result = withHandle(matching: generation) { handle in
            sessionId.withCString { sessionPointer in
                subscriptionId.withCString { subscriptionPointer in
                    kodosi_terminal_refresh(
                        handle,
                        sessionPointer,
                        subscriptionPointer,
                        subscriptionGeneration
                    )
                }
            }
        }
        if result != Int32(KODOSI_FFI_OK) {
            Logger.terminal.error(
                "terminal_refresh failed for \(sessionId): \(result ?? Int32(KODOSI_FFI_RUNTIME_STOPPED))"
            )
        }
    }

    func disconnectTerminal(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64
    ) {
        guard let removal = terminalSubscriptions.removeExactTakingCompletion(
            sessionId: sessionId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration
        ) else {
            Logger.terminal.debug(
                "Ignored stale terminal disconnect for \(sessionId), subscription \(subscriptionId)"
            )
            return
        }
        removal.pendingCompletion?(
            false,
            String(localized: "The terminal connection was cancelled.")
        )
        guard let generation = handleSlot.currentGeneration() else { return }
        disconnectRustTerminal(
            sessionId: sessionId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration,
            handleGeneration: generation
        )
    }

    private func disconnectRustTerminal(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64,
        handleGeneration: UInt64
    ) {
        _ = withHandle(matching: handleGeneration) { handle in
            sessionId.withCString { sessionPointer in
                subscriptionId.withCString { subscriptionPointer in
                    kodosi_terminal_disconnect(
                        handle,
                        sessionPointer,
                        subscriptionPointer,
                        subscriptionGeneration
                    )
                }
            }
        }
    }

    private static let terminalConnectBackoff: [Duration] = [
        .milliseconds(5), .milliseconds(10), .milliseconds(20), .milliseconds(40),
    ]

    private func hasTerminalConnection(
        _ sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64
    ) -> Bool {
        terminalSubscriptions.contains(
            sessionId: sessionId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration
        )
    }

    private func completeTerminalConnection(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64,
        result: Int32,
        message: String? = nil
    ) {
        let succeeded = result == Int32(KODOSI_FFI_OK)
        guard let completion = terminalSubscriptions.takeCompletion(
            sessionId: sessionId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration,
            keepConnection: succeeded
        ) else {
            Logger.terminal.debug(
                "Ignored stale terminal completion for \(sessionId), subscription \(subscriptionId), generation \(subscriptionGeneration)"
            )
            return
        }
        let failureMessage = message
            ?? (succeeded
                ? nil
                : String(localized: "The terminal could not connect (\(result))."))
        completion(succeeded, failureMessage)
    }

    private static func resolve(_ ud: UnsafeMutableRawPointer?) -> RuntimeHandle? {
        guard let ud else { return nil }
        return Unmanaged<RuntimeHandle>.fromOpaque(ud).takeUnretainedValue()
    }

    private static let onEvent: kodosi_event_cb_t = { json, len, ud in
        guard len <= KODOSI_MAX_FRAME_BYTES, let json, let runtime = resolve(ud) else { return }
        if !runtime.inbox.push(Data(bytes: json, count: Int(len))) {
            runtime.inbox.close(overflowed: true)
        }
    }

    private static let onTerminalData: kodosi_terminal_data_cb_t = { sid, subscriptionId, subscriptionGeneration, _, bytes, len, ud in
        guard let sid, let subscriptionId, let bytes, let self_ = resolve(ud) else { return }
        let sessionId = String(cString: sid)
        let subscriptionIdValue = String(cString: subscriptionId)

        let connection = self_.terminalSubscriptions.connection(
            sessionId: sessionId,
            subscriptionId: subscriptionIdValue,
            subscriptionGeneration: subscriptionGeneration
        )

        guard let connection else {
            Logger.terminal.debug(
                "Ignored stale terminal data for \(sessionId), subscription \(subscriptionIdValue)"
            )
            return
        }
        connection.onData(Data(bytes: bytes, count: Int(len)))
    }

    private static let onTerminalControl: kodosi_terminal_control_cb_t = { sid, subscriptionId, subscriptionGeneration, json, len, ud in
        guard len <= KODOSI_TERMINAL_CONTROL_MAX_BYTES,
              let sid, let subscriptionId, let json, let self_ = resolve(ud)
        else { return }
        let sessionId = String(cString: sid)
        let subscriptionIdValue = String(cString: subscriptionId)

        let connection = self_.terminalSubscriptions.connection(
            sessionId: sessionId,
            subscriptionId: subscriptionIdValue,
            subscriptionGeneration: subscriptionGeneration
        )

        guard let connection else { return }
        connection.onControl(Data(bytes: json, count: Int(len)))
    }

    private static let onTerminalCheckpoint: kodosi_terminal_checkpoint_cb_t = { sid, subscriptionId, generation, nextSequence, rows, cols, bytes, len, ud in
        guard rows > 0, cols > 0,
              len > 0, len <= KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
              let sid, let subscriptionId, let bytes, let self_ = resolve(ud)
        else { return Int32(KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED) }
        let sessionId = String(cString: sid)
        let subscriptionIdValue = String(cString: subscriptionId)
        guard let connection = self_.terminalSubscriptions.connection(
            sessionId: sessionId,
            subscriptionId: subscriptionIdValue,
            subscriptionGeneration: generation
        ) else { return Int32(KODOSI_FFI_STALE_SUBSCRIPTION) }
        return connection.onSemanticCheckpoint(.init(
            bytes: Data(bytes: bytes, count: Int(len)),
            nextSequence: nextSequence,
            rows: rows,
            cols: cols
        ))
    }

    private static let onTerminalConnectResult: kodosi_terminal_connect_result_cb_t = { sid, subscriptionId, subscriptionGeneration, result, ud in
        guard let sid, let subscriptionId, let self_ = resolve(ud) else { return }
        self_.completeTerminalConnection(
            sessionId: String(cString: sid),
            subscriptionId: String(cString: subscriptionId),
            subscriptionGeneration: subscriptionGeneration,
            result: result
        )
    }
}

extension RuntimeHandle {
    func connectTerminal(sessionId: String, connection: TerminalSubscriptionRegistry.Connection) {
        let subscriptionId = connection.subscriptionId
        let subscriptionGeneration = connection.subscriptionGeneration
        guard terminalSubscriptions.install(connection, sessionId: sessionId) else {
            Logger.terminal.debug(
                "Ignored stale terminal connect for \(sessionId), generation \(subscriptionGeneration)"
            )
            return
        }
        let activeGeneration = handleSlot.currentGeneration()
        guard let activeGeneration else {
            completeTerminalConnection(
                sessionId: sessionId,
                subscriptionId: subscriptionId,
                subscriptionGeneration: subscriptionGeneration,
                result: Int32(KODOSI_FFI_RUNTIME_STOPPED),
                message: String(localized: "Kodosi is not ready.")
            )
            Logger.terminal.error("connectTerminal: runtime handle is nil for \(sessionId)")
            return
        }

        Task {
            await self.establishTerminalConnection(
                sessionId: sessionId,
                subscriptionId: subscriptionId,
                subscriptionGeneration: subscriptionGeneration,
                handleGeneration: activeGeneration
            )
        }
    }

    private func establishTerminalConnection(
        sessionId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64,
        handleGeneration activeGeneration: UInt64
    ) async {
        for attempt in 0 ... Self.terminalConnectBackoff.count {
            guard hasTerminalConnection(
                sessionId,
                subscriptionId: subscriptionId,
                subscriptionGeneration: subscriptionGeneration
            ) else { return }
            let result = withHandle(matching: activeGeneration) { handle in
                sessionId.withCString { sessionPointer in
                    subscriptionId.withCString { subscriptionPointer in
                        kodosi_terminal_connect(
                            handle,
                            sessionPointer,
                            subscriptionPointer,
                            subscriptionGeneration
                        )
                    }
                }
            }
            if result == Int32(KODOSI_FFI_OK) {
                guard hasTerminalConnection(
                    sessionId,
                    subscriptionId: subscriptionId,
                    subscriptionGeneration: subscriptionGeneration
                ) else {
                    disconnectRustTerminal(
                        sessionId: sessionId,
                        subscriptionId: subscriptionId,
                        subscriptionGeneration: subscriptionGeneration,
                        handleGeneration: activeGeneration
                    )
                    return
                }
                Logger.terminal.info("terminal_connect accepted for \(sessionId)")
                return
            }
            if result == Int32(KODOSI_FFI_BUSY),
               attempt < Self.terminalConnectBackoff.count
            {
                try? await Task.sleep(for: Self.terminalConnectBackoff[attempt])
                continue
            }
            if let result {
                Logger.terminal.error("terminal_connect failed for \(sessionId): \(result)")
                completeTerminalConnection(
                    sessionId: sessionId,
                    subscriptionId: subscriptionId,
                    subscriptionGeneration: subscriptionGeneration,
                    result: result,
                    message: String(localized: "The terminal could not connect (\(result)).")
                )
            } else {
                Logger.terminal.error("terminal_connect aborted after runtime restart for \(sessionId)")
                completeTerminalConnection(
                    sessionId: sessionId,
                    subscriptionId: subscriptionId,
                    subscriptionGeneration: subscriptionGeneration,
                    result: Int32(KODOSI_FFI_RUNTIME_STOPPED),
                    message: String(localized: "Kodosi started again before the terminal connected.")
                )
            }
            return
        }
    }
}
