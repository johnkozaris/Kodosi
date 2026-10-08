import AppKit

extension AppTerminalView {
    func setupTrackingArea() {
        let options: NSTrackingArea.Options = [
            .mouseEnteredAndExited,
            .mouseMoved,
            .inVisibleRect,
            .activeAlways,
        ]
        let area = NSTrackingArea(
            rect: bounds,
            options: options,
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
    }

    override open func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach { removeTrackingArea($0) }
        setupTrackingArea()
    }

    override open var acceptsFirstResponder: Bool {
        true
    }

    override open func becomeFirstResponder() -> Bool {
        let result = super.becomeFirstResponder()
        if result {
            let focused = window?.isKeyWindow == true
            core.setFocus(focused)
            onFocusChange?(focused)
        }
        return result
    }

    override open func resignFirstResponder() -> Bool {
        let result = super.resignFirstResponder()
        if result {
            core.setFocus(false)
            onFocusChange?(false)
        }
        return result
    }

    override open func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        removeWindowObservers()
        if window != nil {
            if surface == nil {
                core.rebuildIfReady()
            } else {
                core.synchronizeDisplayID()
                core.synchronizeMetrics()
            }
            updateMetalLayerMetrics()
            updateColorScheme()
            core.startRendering()
            core.requestImmediateTick()

            NotificationCenter.default.addObserver(
                self,
                selector: #selector(windowDidBecomeKey),
                name: NSWindow.didBecomeKeyNotification,
                object: window
            )
            NotificationCenter.default.addObserver(
                self,
                selector: #selector(windowDidResignKey),
                name: NSWindow.didResignKeyNotification,
                object: window
            )
            NotificationCenter.default.addObserver(
                self,
                selector: #selector(windowDidChangeScreen),
                name: NSWindow.didChangeScreenNotification,
                object: window
            )
        } else {
            core.stopRendering()
            core.setFocus(false)
            onFocusChange?(false)
        }
    }

    @objc func windowDidBecomeKey(_: Notification) {
        let focused = window?.isKeyWindow == true
            && window?.firstResponder === self
        core.setFocus(focused)
        onFocusChange?(focused)
    }

    @objc func windowDidResignKey(_: Notification) {
        core.setFocus(false)
        onFocusChange?(false)
    }

    @objc func windowDidChangeScreen(_ notification: Notification) {
        guard let changedWindow = notification.object as? NSWindow,
              changedWindow === window
        else { return }
        core.synchronizeDisplayID()
        DispatchQueue.main.async { [weak self, weak changedWindow] in
            guard let self,
                  let changedWindow,
                  changedWindow === window
            else { return }
            updateMetalLayerMetrics()
            core.synchronizeMetrics()
            core.requestImmediateTick()
        }
    }

    private func removeWindowObservers() {
        NotificationCenter.default.removeObserver(
            self,
            name: NSWindow.didBecomeKeyNotification,
            object: nil
        )
        NotificationCenter.default.removeObserver(
            self,
            name: NSWindow.didResignKeyNotification,
            object: nil
        )
        NotificationCenter.default.removeObserver(
            self,
            name: NSWindow.didChangeScreenNotification,
            object: nil
        )
    }

    override open func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        core.fitToSize()
        core.requestImmediateTick()
    }

    override open func layout() {
        super.layout()
        core.fitToSize()
        core.requestImmediateTick()
    }

    override open func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        core.synchronizeDisplayID()
        updateMetalLayerMetrics()
        core.fitToSize()
        core.requestImmediateTick()
    }

    public func fitToSize() {
        core.fitToSize()
    }

    func updateMetalLayerMetrics() {
        guard bounds.width > 0, bounds.height > 0 else { return }
        let scale = core.scaleFactor()
        layer?.contentsScale = scale
        if let metal = layer as? CAMetalLayer {
            metal.drawableSize = CGSize(
                width: bounds.width * scale,
                height: bounds.height * scale
            )
        }
        metalLayer?.contentsScale = scale
        metalLayer?.drawableSize = CGSize(
            width: bounds.width * scale,
            height: bounds.height * scale
        )
    }

    func enforceMetalLayerScale() {
        let scale = core.scaleFactor()
        if let layer, layer.contentsScale != scale {
            layer.contentsScale = scale
        }
        if let metalLayer, metalLayer.contentsScale != scale {
            metalLayer.contentsScale = scale
        }
    }

    override open func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColorScheme()
    }

    func updateColorScheme() {
        let scheme: TerminalColorScheme = switch effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) {
        case .darkAqua: .dark
        default: .light
        }
        surface?.setColorScheme(scheme.ghosttyValue)
        guard configuration?.hasSurfaceConfiguration != true else { return }
        controller?.setColorScheme(scheme)
    }
}
