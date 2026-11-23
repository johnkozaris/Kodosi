import Combine
import Foundation
import GhosttyTerminal
import KodosiKit
import Observation
import Synchronization
#if os(iOS)
    import UIKit
#endif

public typealias TerminalFocusRevoker = @MainActor () -> Void

private final class TerminalInputAuthority: Sendable {
    private let allowed = Mutex(true)

    var isAllowed: Bool {
        allowed.withLock { $0 }
    }

    func update(_ value: Bool) {
        allowed.withLock { $0 = value }
    }
}

private final class TerminalSurfaceGenerationCell: Sendable {
    private let generation = Mutex<UInt64>(0)

    var value: UInt64 {
        generation.withLock { $0 }
    }

    @discardableResult
    func advance() -> UInt64 {
        generation.withLock {
            $0 &+= 1
            return $0
        }
    }
}

final class TerminalAccessibilityUpdateRelay: Sendable {
    typealias Handler = @MainActor @Sendable () -> Bool

    private struct State {
        var signalled = false
        var workerRunning = false
        var handlers: [UUID: Handler] = [:]
    }

    private static let refreshAttempts = 40
    private static let refreshInterval = Duration.milliseconds(50)

    private let state = Mutex(State())
    @MainActor private var lastRefresh: ContinuousClock.Instant?

    func register(_ handler: @escaping Handler) -> UUID {
        let token = UUID()
        state.withLock {
            $0.handlers[token] = handler
        }
        return token
    }

    func unregister(_ token: UUID) {
        _ = state.withLock {
            $0.handlers.removeValue(forKey: token)
        }
    }

    func signal() {
        let shouldStartWorker = state.withLock {
            guard !$0.handlers.isEmpty else { return false }
            $0.signalled = true
            guard !$0.workerRunning else { return false }
            $0.workerRunning = true
            return true
        }
        guard shouldStartWorker else { return }

        Task { @MainActor [weak self] in
            await self?.runWorker()
        }
    }

    @MainActor
    private func runWorker() async {
        var attempt = 0
        var wait = lastRefresh.map { Self.refreshInterval - $0.duration(to: .now) } ?? .zero
        while true {
            if wait > .zero, await (try? Task.sleep(for: wait)) == nil {
                break
            }
            lastRefresh = .now
            let (fresh, current) = takeSignal()
            if fresh {
                attempt = 0
            }
            var viewportChanged = false
            for handler in current {
                viewportChanged = handler() || viewportChanged
            }
            attempt += 1
            if current.isEmpty || viewportChanged || attempt >= Self.refreshAttempts {
                if stopWorker() {
                    return
                }
                attempt = 0
            }
            wait = Self.refreshInterval
        }
        state.withLock { $0.workerRunning = false }
    }

    private func takeSignal() -> (fresh: Bool, handlers: [Handler]) {
        state.withLock {
            defer { $0.signalled = false }
            return ($0.signalled, Array($0.handlers.values))
        }
    }

    private func stopWorker() -> Bool {
        state.withLock {
            guard !$0.signalled else { return false }
            $0.workerRunning = false
            return true
        }
    }
}

#if os(iOS)
    @MainActor
    @Observable
    public final class TerminalCursor {
        public internal(set) var row: ClosedRange<CGFloat>?
    }
#endif

@MainActor
public final class TerminalRendererSession: ObservableObject {
    public private(set) var nativeSurfaceGeneration: UInt64 = 0
    #if os(iOS)
        @Published public private(set) var rowMetrics: TerminalRowMetrics?
        public let cursor = TerminalCursor()
    #endif

    public var pastePicture: ((Data) -> Void)?
    private let backend: InMemoryTerminalSession
    private let inputAuthority: TerminalInputAuthority
    private let accessibilityUpdateRelay: TerminalAccessibilityUpdateRelay
    let viewState: TerminalViewState

    public init(
        write: @escaping @Sendable (Data) -> Void,
        resize: @escaping @Sendable (TerminalViewport, UInt64) -> Void,
        onSurfaceAttached: @escaping @MainActor @Sendable (UInt64) -> Void = { _ in },
    ) {
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
                        cellHeightPixels: viewport.cellHeightPixels,
                    ),
                    surfaceGeneration.value,
                )
            },
        )
        self.backend = backend
        let configuration = TerminalSurfaceOptions(session: backend)
        viewState = TerminalViewState(
            controller: TerminalController.shared,
            configuration: configuration,
        )
        viewState.onSurfaceAttached = { [weak self] in
            guard let self else { return }
            let generation = surfaceGeneration.advance()
            nativeSurfaceGeneration = generation
            onSurfaceAttached(generation)
        }
        #if os(iOS)
            viewState.$surfaceSize
                .map { size in
                    size.map {
                        TerminalRowMetrics(rows: $0.rows, heightPixels: $0.heightPixels, cellHeightPixels: $0.cellHeightPixels)
                    }
                }
                .removeDuplicates()
                .assign(to: &$rowMetrics)
        #endif
    }

    public var currentViewport: TerminalViewport? {
        backend.viewport.map {
            TerminalViewport(
                columns: $0.columns,
                rows: $0.rows,
                cellWidthPixels: $0.cellWidthPixels,
                cellHeightPixels: $0.cellHeightPixels,
            )
        }
    }

    public func setAllowsInput(_ allowsInput: Bool) {
        inputAuthority.update(allowsInput)
    }

    #if os(iOS)
        weak var inputView: UITerminalView?

        @discardableResult
        public func sendKeyPress(_ usage: UIKeyboardHIDUsage, modifiers: UIKeyModifierFlags = []) -> Bool {
            guard inputAuthority.isAllowed, let inputView else { return false }
            inputView.sendKeyPress(usage, modifiers: modifiers)
            return true
        }

        @discardableResult
        public func pasteText(_ text: String) -> Bool {
            guard inputAuthority.isAllowed, let inputView else { return false }
            return inputView.pasteText(text)
        }
    #endif

    public nonisolated func readViewportText() -> String? {
        backend.readViewportText()
    }

    func registerAccessibilityUpdateHandler(
        _ handler: @escaping @MainActor @Sendable () -> Bool,
    ) -> UUID {
        accessibilityUpdateRelay.register(handler)
    }

    func unregisterAccessibilityUpdateHandler(_ token: UUID) {
        accessibilityUpdateRelay.unregister(token)
    }

    func signalAccessibilityViewportChange() {
        accessibilityUpdateRelay.signal()
    }

    public func setStyle(_ style: TerminalStyle) {
        objectWillChange.send()
        viewState.configuration = TerminalSurfaceOptions(
            session: backend,
            fontSize: style.ghosttyFontSize,
            terminalConfiguration: Self.makeConfiguration(style: style),
            scrollbackLimitBytes: style.scrollback.bytes,
        )
    }

    @discardableResult
    public nonisolated func receive(_ data: Data) -> Bool {
        guard backend.receive(data) else { return false }
        accessibilityUpdateRelay.signal()
        return true
    }

    @discardableResult
    public nonisolated func applyHostResize(_ viewport: TerminalViewport) -> Bool {
        guard backend.applyHostResize(InMemoryTerminalViewport(
            columns: viewport.columns,
            rows: viewport.rows,
            cellWidthPixels: viewport.cellWidthPixels,
            cellHeightPixels: viewport.cellHeightPixels,
        )) else { return false }
        accessibilityUpdateRelay.signal()
        return true
    }

    @discardableResult
    public nonisolated func restoreCheckpointSynchronously(_ snapshot: Data, console: Bool) -> Bool {
        guard backend.restoreCheckpointSynchronously(snapshot, console: console) else { return false }
        accessibilityUpdateRelay.signal()
        return true
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

    private nonisolated static let hostManagedPolicyOverrides: [(key: String, value: String)] = [
        ("clipboard-read", "allow"),
        ("clipboard-write", "allow"),
        ("image-storage-limit", String(KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES)),
    ]

    private static func makeConfiguration(style: TerminalStyle) -> TerminalConfiguration {
        let palette = style.palette
        return TerminalConfiguration {
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
            #if os(macOS)
                $0.withCustom("mouse-hide-while-typing", "true")
            #endif
            $0.withCustom(
                "adjust-cell-height",
                "\(style.lineHeightAdjustment)%",
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
