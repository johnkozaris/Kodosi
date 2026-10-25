#if os(iOS)
import GhosttyKit
import UIKit

@MainActor
open class UITerminalView: UIView {
    let core = TerminalSurfaceCoordinator()
    lazy var inputHandler = TerminalTextInputHandler(view: self)
    lazy var editMenuInteraction = UIEditMenuInteraction(delegate: self)
    weak var textInputDelegate: (any UITextInputDelegate)?
    var hardwareKeyHandled = false
    var pressedKeyUsages: Set<UInt16> = []
    var keyRepeatTimer: Timer?
    var keyRepeatUsage: UInt16?
    var keyRepeatStroke: TerminalKeyStroke?
    var momentumDisplayLink: CADisplayLink?
    var momentumVelocity: CGPoint = .zero
    var activePointerButton: ghostty_input_mouse_button_e?
    var touchSelectionModifiers: TerminalInputModifiers?
    weak var observedWindow: UIWindow?
    weak var observedScene: UIWindowScene?
    public var onFocusChange: ((Bool) -> Void)?
    public var onCursorRowChange: ((ClosedRange<CGFloat>) -> Void)?
    var reportedCursorRow: ClosedRange<CGFloat>?

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

    override public init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func commonInit() {
        backgroundColor = .clear
        isOpaque = false
        clipsToBounds = true
        isAccessibilityElement = true
        accessibilityLabel = "Terminal"
        accessibilityTraits = [.allowsDirectInteraction, .updatesFrequently]

        core.isAttached = { [weak self] in self?.window != nil }
        core.scaleFactor = { [weak self] in
            Double(self?.displayScale ?? 2)
        }
        core.viewSize = { [weak self] in
            guard let self else { return (0, 0) }
            return (bounds.width, bounds.height)
        }
        core.platformSetup = { [weak self] config in
            guard let self else { return }
            config.platform_tag = GHOSTTY_PLATFORM_IOS
            config.platform = ghostty_platform_u(
                ios: ghostty_platform_ios_s(
                    uiview: Unmanaged.passUnretained(self).toOpaque()
                )
            )
        }
        core.onMetricsUpdate = { [weak self] in
            self?.updateSurfaceLayers()
        }
        core.onPostRender = { [weak self] in
            self?.updateSurfaceLayers()
            self?.reportCursorRow()
        }
        core.onCellSizeDidChange = { [weak self] in
            self?.inputHandler.notifyGeometryDidChange()
        }
        core.onSurfaceFreed = { [weak self] in
            self?.removeSurfaceLayers()
        }

        registerForTraitChanges(
            [UITraitUserInterfaceStyle.self, UITraitDisplayScale.self]
        ) { (view: UITerminalView, _: UITraitCollection) in
            view.traitsDidChange()
        }
        setupInteractions()
    }

    var displayScale: CGFloat {
        let scale = traitCollection.displayScale
        if scale > 0 { return scale }
        return window?.windowScene?.screen.scale ?? 2
    }

    open func selectionMenuElements() -> [UIMenuElement] {
        var elements: [UIMenuElement] = []
        if surface?.hasSelection() == true {
            elements.append(UIAction(
                title: "Copy",
                image: UIImage(systemName: "doc.on.doc")
            ) { [weak self] _ in
                self?.copySelectedTextToPasteboard()
            })
        }
        if surface != nil, UIPasteboard.general.hasStrings || UIPasteboard.general.hasURLs {
            elements.append(UIAction(
                title: "Paste",
                image: UIImage(systemName: "doc.on.clipboard")
            ) { [weak self] _ in
                self?.paste(nil)
            })
        }
        return elements
    }

    func presentSelectionMenu(at point: CGPoint) {
        guard !selectionMenuElements().isEmpty else { return }
        editMenuInteraction.presentEditMenu(
            with: UIEditMenuConfiguration(identifier: nil, sourcePoint: point)
        )
    }

    @discardableResult
    open func pasteText(_ text: String) -> Bool {
        TerminalDebugLog.log(
            .input,
            "paste text bytes=\(text.utf8.count) lines=\(TerminalInputText.lineCount(in: text))"
        )
        return surface?.sendText(text) ?? false
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

    override open func copy(_: Any?) {
        _ = copySelectedTextToPasteboard()
    }

    override open func paste(_: Any?) {
        if let text = UIPasteboard.general.string {
            TerminalDebugLog.log(
                .input,
                "paste binding bytes=\(text.utf8.count) lines=\(TerminalInputText.lineCount(in: text))"
            )
        }
        _ = surface?.performBindingAction("paste_from_clipboard")
    }

    override open func selectAll(_: Any?) {
        _ = surface?.performBindingAction("select_all")
    }

    override open func canPerformAction(
        _ action: Selector,
        withSender sender: Any?
    ) -> Bool {
        switch action {
        case #selector(copy(_:)):
            return surface?.hasSelection() == true
        case #selector(paste(_:)):
            return surface != nil
                && (UIPasteboard.general.hasStrings || UIPasteboard.general.hasURLs)
        case #selector(selectAll(_:)):
            return surface != nil
        default:
            return super.canPerformAction(action, withSender: sender)
        }
    }

    override open var accessibilityValue: String? {
        get { configuration?.session.readViewportText() }
        set {}
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }
}
#endif
