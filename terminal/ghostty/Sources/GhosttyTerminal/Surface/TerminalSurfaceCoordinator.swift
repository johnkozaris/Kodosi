import Foundation
import GhosttyKit

@MainActor
final class TerminalSurfaceCoordinator {
    weak var delegate: (any TerminalSurfaceViewDelegate)? {
        didSet { bridge.delegate = delegate }
    }

    var controller: TerminalController? {
        didSet {
            guard controller !== oldValue else { return }
            rebuildIfReady(removingBridgeFrom: oldValue)
        }
    }

    var configuration: TerminalSurfaceOptions? {
        didSet {
            switch (oldValue, configuration) {
            case (nil, nil):
                return
            case let (old?, new?) where new.isEquivalent(to: old):
                return
            case let (old?, new?) where !new.requiresSurfaceRebuild(comparedTo: old):
                applySurfaceConfiguration()
            default:
                rebuildIfReady()
            }
        }
    }

    var surface: TerminalSurface?
    let bridge = TerminalCallbackBridge()
    private var attachedSession: InMemoryTerminalSession?
    private var attachedRawSurface: ghostty_surface_t?

    var isAttached: () -> Bool = { false }
    var displayIDProvider: () -> UInt32? = { nil }
    var applyDisplayID: (TerminalSurface, UInt32) -> Void = { surface, displayID in
        surface.setDisplayID(displayID)
    }

    var scaleFactor: () -> Double = { 2.0 }
    var viewSize: () -> (width: Double, height: Double) = { (0, 0) }
    var platformSetup: ((inout ghostty_surface_config_s) -> Void)?
    var onMetricsUpdate: (() -> Void)?
    var onCellSizeDidChange: (() -> Void)?

    var onPostRender: (() -> Void)?

    private static let minimumFrameInterval: TimeInterval = 1.0 / 60.0

    private var lastMetrics: TerminalViewportMetrics?
    private var isDisplayVisible = true
    private var isApplicationActive = true
    private var isSurfaceFocused = false
    private var pendingImmediateTick = true
    private var lastTickTimestamp: TimeInterval = 0
    private var tickScheduled = false
    private var pendingRebuild = false

    init() {
        bridge.onCellSizeChange = { [weak self] width, height in
            self?.handleCellSizeChange(width: width, height: height)
        }
        bridge.onRenderRequest = { [weak self] in
            self?.requestImmediateTick()
        }
        bridge.canProcessAppWakeup = { [weak self] in
            self?.canRenderFrame == true
        }
        bridge.onAppWakeup = { [weak self] in
            self?.requestImmediateTick()
        }
    }

    func requestImmediateTick() {
        pendingImmediateTick = true
        scheduleTickIfNeeded()
    }

    func startRendering() {
        scheduleTickIfNeeded()
    }

    func stopRendering() {
        tickScheduled = false
    }

    func rebuildIfReady(removingBridgeFrom previousController: TerminalController? = nil) {
        if surface != nil, previousController == nil, !hasValidViewSize {
            pendingRebuild = true
            let size = viewSize()
            TerminalDebugLog.log(
                .lifecycle,
                "surface kept: view size temporarily invalid \(String(format: "%.2f", size.width))x\(String(format: "%.2f", size.height))"
            )
            return
        }
        pendingRebuild = false

        tearDownSurface(removingBridgeFrom: previousController ?? controller)
        guard let controller else {
            TerminalDebugLog.log(.lifecycle, "surface rebuild skipped: missing controller")
            return
        }
        guard let configuration else {
            TerminalDebugLog.log(.lifecycle, "surface rebuild skipped: missing session")
            return
        }
        guard isAttached() else {
            TerminalDebugLog.log(.lifecycle, "surface rebuild skipped: view detached")
            return
        }
        guard hasValidViewSize else {
            let size = viewSize()
            TerminalDebugLog.log(
                .lifecycle,
                "surface rebuild skipped: invalid view size=\(String(format: "%.2f", size.width))x\(String(format: "%.2f", size.height))"
            )
            return
        }

        let scale = scaleFactor()
        TerminalDebugLog.log(
            .lifecycle,
            "surface rebuild scale=\(String(format: "%.2f", scale)) \(configuration.debugSummary)"
        )
        let rawSurface = controller.createSurface(
            bridge: bridge,
            configuration: configuration,
            platformSetup: { [self] config in
                platformSetup?(&config)
                config.scale_factor = scale
            }
        )
        guard let rawSurface else {
            TerminalDebugLog.log(.lifecycle, "surface rebuild failed")
            return
        }

        bridge.rawSurface = rawSurface
        let newSurface = TerminalSurface(rawSurface)
        surface = newSurface
        synchronizeDisplayID(on: newSurface)
        newSurface.setOcclusion(effectiveSurfaceVisible)
        TerminalDebugLog.log(.lifecycle, "surface rebuild succeeded")
        synchronizeMetrics()
        attachInMemorySession(to: rawSurface)
        (delegate as? any TerminalSurfaceLifecycleDelegate)?
            .terminalDidAttachSurface(newSurface)
        requestImmediateTick()
    }

    private func applySurfaceConfiguration() {
        guard let controller,
              let configuration,
              let rawSurface = surface?.rawValue
        else { return }
        guard controller.updateSurface(
            rawSurface,
            bridge: bridge,
            configuration: configuration
        ) else { return }
        synchronizeMetrics()
        requestImmediateTick()
    }

    func synchronizeDisplayID(on target: TerminalSurface? = nil) {
        guard let target = target ?? surface else {
            TerminalDebugLog.log(.lifecycle, "display sync skipped: missing surface")
            return
        }
        guard let displayID = displayIDProvider(), displayID != 0 else {
            TerminalDebugLog.log(.lifecycle, "display sync skipped: missing display ID")
            return
        }
        applyDisplayID(target, displayID)
    }

    func synchronizeMetrics() {
        if pendingRebuild, hasValidViewSize {
            pendingRebuild = false
            rebuildIfReady()
            return
        }

        guard let surface else {
            TerminalDebugLog.log(.metrics, "synchronizeMetrics skipped: missing surface")
            return
        }

        let scale = scaleFactor()
        let size = viewSize()
        guard size.width > 0, size.height > 0 else {
            TerminalDebugLog.log(
                .metrics,
                "synchronizeMetrics skipped: invalid view size=\(String(format: "%.2f", size.width))x\(String(format: "%.2f", size.height))"
            )
            return
        }

        let pixelWidth = UInt32((size.width * scale).rounded(.down))
        let pixelHeight = UInt32((size.height * scale).rounded(.down))
        guard pixelWidth > 0, pixelHeight > 0 else {
            TerminalDebugLog.log(
                .metrics,
                "synchronizeMetrics skipped: invalid pixel size=\(pixelWidth)x\(pixelHeight)"
            )
            return
        }

        TerminalDebugLog.log(
            .metrics,
            "sync view=\(String(format: "%.2f", size.width))x\(String(format: "%.2f", size.height)) scale=\(String(format: "%.2f", scale)) pixels=\(pixelWidth)x\(pixelHeight)"
        )

        surface.setContentScale(x: scale, y: scale)
        surface.setSize(width: pixelWidth, height: pixelHeight)

        guard let surfaceSize = surface.size(),
              surfaceSize.columns > 0, surfaceSize.rows > 0
        else {
            TerminalDebugLog.log(.metrics, "sync missing grid metrics after resize")
            onMetricsUpdate?()
            return
        }

        let metrics = TerminalViewportMetrics(surfaceSize: surfaceSize, scale: scale)
        guard metrics != lastMetrics else {
            TerminalDebugLog.log(
                .metrics,
                "sync unchanged \(metrics.debugSummary)"
            )
            onMetricsUpdate?()
            return
        }

        lastMetrics = metrics
        TerminalDebugLog.log(.metrics, "sync updated \(metrics.debugSummary)")
        configuration?.session.updateViewport(surfaceSize)
        (delegate as? any TerminalSurfaceGridResizeDelegate)?
            .terminalDidResize(surfaceSize)
        onMetricsUpdate?()
    }

    func fitToSize() {
        if surface == nil {
            rebuildIfReady()
        } else {
            synchronizeMetrics()
        }
        if surface != nil {
            requestImmediateTick()
        }
    }

    func setDisplayVisible(_ visible: Bool) {
        guard isDisplayVisible != visible else {
            surface?.setOcclusion(effectiveSurfaceVisible)
            return
        }

        isDisplayVisible = visible
        surface?.setOcclusion(effectiveSurfaceVisible)

        if canRenderFrame {
            requestImmediateTick()
        } else {
            stopRendering()
        }
    }

    func setApplicationActive(_ active: Bool) {
        guard isApplicationActive != active else {
            if active {
                renderImmediately()
            } else {
                stopRendering()
            }
            return
        }

        isApplicationActive = active
        surface?.setOcclusion(effectiveSurfaceVisible)

        if active {
            synchronizeMetrics()
            renderImmediately()
        } else {
            stopRendering()
        }
    }

    func tick(at timestamp: TimeInterval) {
        guard shouldRenderFrame(at: timestamp) else {
            return
        }
        pendingImmediateTick = false
        lastTickTimestamp = timestamp
        TerminalDebugLog.log(.render, "tick")
        surface?.refresh()
        surface?.draw()
        onPostRender?()
    }

    func setFocus(_ focused: Bool) {
        isSurfaceFocused = focused
        requestImmediateTick()
        TerminalDebugLog.log(.lifecycle, "focus=\(focused)")
        surface?.setFocus(focused)
    }

    func freeSurface() {
        TerminalDebugLog.log(.lifecycle, "free surface")
        tearDownSurface(removingBridgeFrom: controller)
    }

    deinit {
        MainActor.assumeIsolated {
            tearDownSurface(removingBridgeFrom: controller)
        }
    }

    func attachInMemorySession(to rawSurface: ghostty_surface_t) {
        attachedSession = configuration?.session
        attachedRawSurface = rawSurface
        attachedSession?.setSurface(rawSurface)
    }

    private func detachInMemorySession() {
        attachedSession?.clearSurface(ifMatches: attachedRawSurface)
        attachedSession = nil
        attachedRawSurface = nil
    }

    private func tearDownSurface(removingBridgeFrom controller: TerminalController?) {
        TerminalDebugLog.log(.lifecycle, "tear down surface")
        pendingRebuild = false
        tickScheduled = false
        detachInMemorySession()
        let hadSurface = surface != nil
        surface?.setFocus(false)
        surface?.free()
        surface = nil
        bridge.rawSurface = nil
        lastMetrics = nil
        pendingImmediateTick = true
        lastTickTimestamp = 0
        controller?.remove(bridge)
        if hadSurface {
            (delegate as? any TerminalSurfaceLifecycleDelegate)?
                .terminalDidDetachSurface()
        }
    }

    private func handleCellSizeChange(width: UInt32, height: UInt32) {
        TerminalDebugLog.log(
            .metrics,
            "cell size changed width=\(width) height=\(height)"
        )
        synchronizeMetrics()
        requestImmediateTick()
        onCellSizeDidChange?()
    }

    private func shouldRenderFrame(at _: TimeInterval) -> Bool {
        guard canRenderFrame else {
            return false
        }
        return pendingImmediateTick || lastTickTimestamp == 0
    }

    private func scheduleTickIfNeeded() {
        guard canRenderFrame else {
            tickScheduled = false
            return
        }
        guard !tickScheduled else {
            return
        }
        tickScheduled = true
        TerminalDebugLog.log(.lifecycle, "tick scheduled")
        let now = Self.monotonicTimestamp()
        let elapsed = now - lastTickTimestamp
        let delay = lastTickTimestamp == 0
            ? 0
            : max(0, Self.minimumFrameInterval - elapsed)
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self else { return }
            tickScheduled = false
            tick(at: Self.monotonicTimestamp())
            if pendingImmediateTick {
                scheduleTickIfNeeded()
            }
        }
    }

    private static func monotonicTimestamp() -> TimeInterval {
        ProcessInfo.processInfo.systemUptime
    }

    private var effectiveSurfaceVisible: Bool {
        isDisplayVisible && isApplicationActive
    }

    private var canRenderFrame: Bool {
        effectiveSurfaceVisible && isAttached()
    }

    private var hasValidViewSize: Bool {
        let size = viewSize()
        return size.width > 0 && size.height > 0
    }

    private func renderImmediately() {
        guard canRenderFrame else {
            tickScheduled = false
            return
        }

        pendingImmediateTick = true
        tickScheduled = false
        tick(at: Self.monotonicTimestamp())
    }
}
