import AppKit
import GhosttyKit

@MainActor
open class AppTerminalView: NSView {
    let core = TerminalSurfaceCoordinator()
    var metalLayer: CAMetalLayer?
    var inputHandler: TerminalKeyEventHandler?
    var lastPerformKeyEvent: TimeInterval?
    public var onFocusChange: ((Bool) -> Void)?

    open weak var delegate: (any TerminalSurfaceViewDelegate)? {
        get { core.delegate }
        set { core.delegate = newValue }
    }

    open var controller: TerminalController? {
        get { core.controller }
        set { core.controller = newValue }
    }

    open var configuration: TerminalSurfaceOptions? {
        get { core.configuration }
        set { core.configuration = newValue }
    }

    open func setSurfaceVisible(_ visible: Bool) {
        core.setDisplayVisible(visible)
    }

    open func tearDownSurface() {
        core.freeSurface()
    }

    var surface: TerminalSurface? {
        core.surface
    }

    override public init(frame: NSRect) {
        super.init(frame: frame)
        commonInit()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func commonInit() {
        wantsLayer = true

        let metal = CAMetalLayer()
        metal.device = MTLCreateSystemDefaultDevice()
        metal.pixelFormat = .bgra8Unorm
        metal.framebufferOnly = true
        metal.contentsScale = NSScreen.main?.backingScaleFactor ?? 2.0
        metal.isOpaque = false
        metal.backgroundColor = NSColor.clear.cgColor
        layer = metal
        metalLayer = metal
        layer?.backgroundColor = NSColor.clear.cgColor

        inputHandler = TerminalKeyEventHandler(view: self)
        setupTrackingArea()

        core.isAttached = { [weak self] in self?.window != nil }
        core.displayIDProvider = { [weak self] in
            self?.window?.screen.flatMap(Self.displayID(for:))
        }
        core.scaleFactor = { [weak self] in
            Double(
                self?.window?.backingScaleFactor
                    ?? NSScreen.main?.backingScaleFactor ?? 2.0
            )
        }
        core.viewSize = { [weak self] in
            guard let self else { return (0, 0) }
            return (bounds.width, bounds.height)
        }
        core.platformSetup = { [weak self] config in
            guard let self else { return }
            config.platform_tag = GHOSTTY_PLATFORM_MACOS
            config.platform = ghostty_platform_u(
                macos: ghostty_platform_macos_s(
                    nsview: Unmanaged.passUnretained(self).toOpaque()
                )
            )
        }
        core.onMetricsUpdate = { [weak self] in
            self?.updateMetalLayerMetrics()
        }
        core.onPostRender = { [weak self] in
            self?.enforceMetalLayerScale()
        }
    }

    static func displayID(for screen: NSScreen) -> UInt32? {
        displayID(in: screen.deviceDescription)
    }

    static func displayID(
        in deviceDescription: [NSDeviceDescriptionKey: Any]
    ) -> UInt32? {
        let key = NSDeviceDescriptionKey("NSScreenNumber")
        guard let number = deviceDescription[key] as? NSNumber else {
            return nil
        }
        let displayID = number.uint32Value
        return displayID == 0 ? nil : displayID
    }

    open func selectionContextMenu() -> NSMenu? {
        guard surface?.hasSelection() == true else { return nil }
        let menu = NSMenu()
        let copyItem = NSMenuItem(
            title: "Copy",
            action: #selector(copy(_:)),
            keyEquivalent: ""
        )
        copyItem.target = self
        menu.addItem(copyItem)
        return menu
    }

    @discardableResult
    open func copySelectedTextToPasteboard() -> Bool {
        guard surface?.hasSelection() == true else {
            return false
        }
        guard surface?.performBindingAction("copy_to_clipboard") == true else {
            return false
        }
        TerminalDebugLog.log(
            .input,
            "selection copied to clipboard"
        )
        return true
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }
}
