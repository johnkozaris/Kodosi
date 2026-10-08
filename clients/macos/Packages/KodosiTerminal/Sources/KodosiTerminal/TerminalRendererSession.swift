import Combine
import Foundation
import GhosttyTerminal
import SwiftUI

private final class TerminalInputAuthority: @unchecked Sendable {
    private let lock = NSLock()
    private var allowed = true

    var isAllowed: Bool {
        lock.withLock { allowed }
    }

    func update(_ value: Bool) {
        lock.withLock { allowed = value }
    }
}

private final class TerminalSurfaceGenerationCell: @unchecked Sendable {
    private let lock = NSLock()
    private var generation: UInt64 = 0

    var value: UInt64 {
        lock.withLock { generation }
    }

    @discardableResult
    func advance() -> UInt64 {
        lock.withLock {
            generation &+= 1
            return generation
        }
    }
}

private final class TerminalAccessibilityUpdateRelay: @unchecked Sendable {
    typealias Handler = @MainActor @Sendable () -> Bool

    private static let refreshAttempts = 40
    private static let refreshInterval = Duration.milliseconds(50)

    private let lock = NSLock()
    private var signalGeneration: UInt64 = 0
    private var workerRunning = false
    private var handlers: [UUID: Handler] = [:]

    func register(_ handler: @escaping Handler) -> UUID {
        let token = UUID()
        lock.withLock {
            handlers[token] = handler
        }
        return token
    }

    func unregister(_ token: UUID) {
        _ = lock.withLock {
            handlers.removeValue(forKey: token)
        }
    }

    func signal() {
        let shouldStartWorker = lock.withLock {
            signalGeneration &+= 1
            guard !workerRunning else { return false }
            workerRunning = true
            return true
        }
        guard shouldStartWorker else { return }

        Task { @MainActor [weak self] in
            await self?.runWorker()
        }
    }

    @MainActor
    private func runWorker() async {
        var generation: UInt64?
        var attempt = 0

        while !Task.isCancelled {
            var state = currentState()
            if generation != state.generation {
                generation = state.generation
                attempt = 0
            }
            if state.handlers.isEmpty {
                if stopWorker(ifCurrent: state.generation) {
                    return
                }
                generation = nil
                continue
            }

            if attempt > 0 {
                try? await Task.sleep(for: Self.refreshInterval)
                guard !Task.isCancelled else { return }
                state = currentState()
                if generation != state.generation {
                    generation = state.generation
                    attempt = 0
                    continue
                }
            }

            var viewportChanged = false
            for handler in state.handlers {
                viewportChanged = handler() || viewportChanged
            }
            attempt += 1
            guard viewportChanged || attempt >= Self.refreshAttempts else {
                continue
            }
            if stopWorker(ifCurrent: state.generation) {
                return
            }
            generation = nil
            attempt = 0
        }
    }

    private func currentState() -> (generation: UInt64, handlers: [Handler]) {
        lock.withLock {
            (signalGeneration, Array(handlers.values))
        }
    }

    private func stopWorker(ifCurrent generation: UInt64) -> Bool {
        lock.withLock {
            guard signalGeneration == generation else { return false }
            workerRunning = false
            return true
        }
    }
}

@MainActor
public final class TerminalRendererSession: ObservableObject {
    @Published public private(set) var nativeSurfaceGeneration: UInt64 = 0

    private let backend: InMemoryTerminalSession
    private let inputAuthority: TerminalInputAuthority
    private let accessibilityUpdateRelay: TerminalAccessibilityUpdateRelay
    private var style: TerminalStyle?
    private var colorScheme: ColorScheme = .light
    let viewState: TerminalViewState

    public init(
        colorScheme: ColorScheme,
        write: @escaping @Sendable (Data) -> Void,
        resize: @escaping @Sendable (TerminalViewport, UInt64) -> Void,
        onSurfaceAttached: @escaping @MainActor @Sendable (UInt64) -> Void = { _ in }
    ) {
        self.colorScheme = colorScheme
        Self.configureDebugLoggingIfRequested()
        let inputAuthority = TerminalInputAuthority()
        let surfaceGeneration = TerminalSurfaceGenerationCell()
        let accessibilityUpdateRelay = TerminalAccessibilityUpdateRelay()
        self.inputAuthority = inputAuthority
        self.accessibilityUpdateRelay = accessibilityUpdateRelay
        let backend = InMemoryTerminalSession(
            write: { data in
                guard inputAuthority.isAllowed else { return }
                write(data)
            },
            resize: { viewport in
                resize(
                    TerminalViewport(
                        columns: viewport.columns,
                        rows: viewport.rows,
                        cellWidthPixels: viewport.cellWidthPixels,
                        cellHeightPixels: viewport.cellHeightPixels
                    ),
                    surfaceGeneration.value
                )
            }
        )
        self.backend = backend
        let configuration = TerminalSurfaceOptions(session: backend)
        viewState = TerminalViewState(
            controller: TerminalController.shared,
            configuration: configuration
        )
        viewState.onSurfaceAttached = { [weak self] in
            guard let self else { return }
            let generation = surfaceGeneration.advance()
            nativeSurfaceGeneration = generation
            onSurfaceAttached(generation)
        }
    }

    public var currentViewport: TerminalViewport? {
        viewState.surfaceSize.map {
            TerminalViewport(
                columns: $0.columns,
                rows: $0.rows,
                cellWidthPixels: $0.cellWidthPixels,
                cellHeightPixels: $0.cellHeightPixels
            )
        }
    }

    public func setAllowsInput(_ allowsInput: Bool) {
        inputAuthority.update(allowsInput)
    }

    public nonisolated func readViewportText() -> String? {
        backend.readViewportText()
    }

    func registerAccessibilityUpdateHandler(
        _ handler: @escaping @MainActor @Sendable () -> Bool
    ) -> UUID {
        accessibilityUpdateRelay.register(handler)
    }

    func unregisterAccessibilityUpdateHandler(_ token: UUID) {
        accessibilityUpdateRelay.unregister(token)
    }

    func signalAccessibilityViewportChange() {
        accessibilityUpdateRelay.signal()
    }

    #if DEBUG
        func sendInputForTest(_ data: Data) {
            backend.sendInput(data)
        }
    #endif

    public func setStyle(_ style: TerminalStyle) {
        self.style = style
        applyStyle()
    }

    public func adopt(colorScheme: ColorScheme) {
        self.colorScheme = colorScheme
        applyStyle()
    }

    @discardableResult
    public nonisolated func receive(_ data: Data) -> Bool {
        guard backend.receive(data) else { return false }
        accessibilityUpdateRelay.signal()
        return true
    }

    @discardableResult
    public nonisolated func restoreCheckpointSynchronously(_ data: Data) -> Bool {
        guard backend.restoreCheckpointSynchronously(data) else { return false }
        accessibilityUpdateRelay.signal()
        return true
    }

    private func applyStyle() {
        guard let style else { return }
        let palette = colorScheme == .dark ? style.darkPalette : style.lightPalette
        objectWillChange.send()
        viewState.configuration = TerminalSurfaceOptions(
            session: backend,
            fontSize: style.fontSize,
            terminalConfiguration: Self.makeConfiguration(
                style: style,
                palette: palette
            ),
            scrollbackLimitBytes: style.scrollback.bytes
        )
    }

    private static func configureDebugLoggingIfRequested() {
        guard ProcessInfo.processInfo.environment["KODOSI_TERMINAL_DEBUG"] == "1" else {
            return
        }
        TerminalDebugLog.sink = { message in
            let line = Data("\(message)\n".utf8)
            try? FileHandle.standardError.write(contentsOf: line)
        }
        TerminalDebugLog.enable([.lifecycle, .metrics])
    }

    nonisolated static let hostManagedPolicyOverrides: [(key: String, value: String)] = [
        ("clipboard-read", "deny"),
        ("clipboard-write", "deny"),
        ("image-storage-limit", "0"),
        ("mouse-shift-capture", "never"),
    ]

    private static func makeConfiguration(
        style: TerminalStyle,
        palette: TerminalPalette
    ) -> TerminalConfiguration {
        TerminalConfiguration {
            $0.withFontFamily(style.fontFamily)
            switch style.cursorStyle {
            case .block:
                $0.withCursorStyle(.block)
            case .bar:
                $0.withCursorStyle(.bar)
            case .underline:
                $0.withCursorStyle(.underline)
            }
            $0.withCursorStyleBlink(style.cursorBlink)
            $0.withWindowPaddingX(style.paddingX)
            $0.withWindowPaddingY(style.paddingY)
            for override in hostManagedPolicyOverrides {
                $0.withCustom(override.key, override.value)
            }
            $0.withCustom(
                "adjust-cell-height",
                "\(style.lineHeightAdjustment)%"
            )
            $0.withBackground(palette.background)
            $0.withForeground(palette.foreground)
            $0.withCursorColor(palette.cursor)
            $0.withSelectionBackground(palette.selectionBackground)
            $0.withSelectionForeground(palette.selectionForeground)
            for (index, color) in palette.ansiColors.enumerated() {
                $0.withPalette(index, color: color)
            }
            $0.withMinimumContrast(style.minimumContrast)
        }
    }
}
