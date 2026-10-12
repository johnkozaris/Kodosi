#if os(iOS)
import UIKit

extension UITerminalView {
    override open var canBecomeFirstResponder: Bool {
        true
    }

    @discardableResult
    override open func becomeFirstResponder() -> Bool {
        let result = super.becomeFirstResponder()
        if result {
            updateFocus()
        }
        return result
    }

    @discardableResult
    override open func resignFirstResponder() -> Bool {
        let result = super.resignFirstResponder()
        if result {
            releasePressedKeys()
            core.setFocus(false)
            onFocusChange?(false)
        }
        return result
    }

    func updateFocus() {
        let focused = isFirstResponder && window?.isKeyWindow == true
        core.setFocus(focused)
        onFocusChange?(focused)
    }

    override open func didMoveToWindow() {
        super.didMoveToWindow()
        observeWindow()
        if window != nil {
            if surface == nil {
                core.rebuildIfReady()
            } else {
                core.synchronizeMetrics()
            }
            updateSurfaceLayers()
            updateColorScheme()
            core.startRendering()
            core.requestImmediateTick()
        } else {
            stopMomentumScrolling(sendTerminalEndEvent: false)
            releasePressedKeys()
            core.stopRendering()
            core.setFocus(false)
            onFocusChange?(false)
        }
    }

    private func observeWindow() {
        let center = NotificationCenter.default
        if let observedWindow {
            center.removeObserver(self, name: UIWindow.didBecomeKeyNotification, object: observedWindow)
            center.removeObserver(self, name: UIWindow.didResignKeyNotification, object: observedWindow)
        }
        if let observedScene {
            center.removeObserver(self, name: UIScene.didActivateNotification, object: observedScene)
            center.removeObserver(self, name: UIScene.didEnterBackgroundNotification, object: observedScene)
        }
        observedWindow = window
        observedScene = window?.windowScene
        if let window {
            center.addObserver(
                self,
                selector: #selector(windowKeyStateDidChange(_:)),
                name: UIWindow.didBecomeKeyNotification,
                object: window
            )
            center.addObserver(
                self,
                selector: #selector(windowKeyStateDidChange(_:)),
                name: UIWindow.didResignKeyNotification,
                object: window
            )
        }
        if let scene = observedScene {
            center.addObserver(
                self,
                selector: #selector(sceneDidActivate(_:)),
                name: UIScene.didActivateNotification,
                object: scene
            )
            center.addObserver(
                self,
                selector: #selector(sceneDidEnterBackground(_:)),
                name: UIScene.didEnterBackgroundNotification,
                object: scene
            )
            core.setApplicationActive(
                scene.activationState == .foregroundActive
                    || scene.activationState == .foregroundInactive
            )
        }
    }

    @objc func windowKeyStateDidChange(_: Notification) {
        updateFocus()
    }

    @objc func sceneDidActivate(_: Notification) {
        TerminalDebugLog.log(.lifecycle, "scene did activate")
        updateSurfaceLayers()
        updateColorScheme()
        core.setApplicationActive(true)
    }

    @objc func sceneDidEnterBackground(_: Notification) {
        TerminalDebugLog.log(.lifecycle, "scene did enter background")
        stopMomentumScrolling(sendTerminalEndEvent: false)
        releasePressedKeys()
        core.setApplicationActive(false)
    }

    override open func layoutSubviews() {
        super.layoutSubviews()
        updateSurfaceLayers()
        core.fitToSize()
    }

    public func fitToSize() {
        core.fitToSize()
    }

    func traitsDidChange() {
        updateSurfaceLayers()
        updateColorScheme()
        core.fitToSize()
    }

    func updateSurfaceLayers() {
        let scale = displayScale
        if contentScaleFactor != scale {
            contentScaleFactor = scale
        }
        for surfaceLayer in surfaceLayers {
            if surfaceLayer.frame != bounds {
                surfaceLayer.frame = bounds
            }
            if surfaceLayer.contentsScale != scale {
                surfaceLayer.contentsScale = scale
            }
        }
    }

    func removeSurfaceLayers() {
        surfaceLayers.forEach { $0.removeFromSuperlayer() }
    }

    private var surfaceLayers: [CALayer] {
        (layer.sublayers ?? []).filter {
            NSStringFromClass(type(of: $0)) == "IOSurfaceLayer"
        }
    }

    func updateColorScheme() {
        let scheme: TerminalColorScheme = traitCollection.userInterfaceStyle == .dark
            ? .dark
            : .light
        surface?.setColorScheme(scheme.ghosttyValue)
        guard configuration?.hasSurfaceConfiguration != true else { return }
        controller?.setColorScheme(scheme)
    }
}
#endif
