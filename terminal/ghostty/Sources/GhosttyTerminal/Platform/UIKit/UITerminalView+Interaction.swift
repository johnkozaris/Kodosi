#if os(iOS)
import GhosttyKit
import UIKit

extension UITerminalView: UIGestureRecognizerDelegate, @preconcurrency UIEditMenuInteractionDelegate {
    private static let scrollDecelerationRate = UIScrollView.DecelerationRate.normal.rawValue
    private static let minimumMomentumVelocity: CGFloat = 20

    func setupInteractions() {
        addInteraction(editMenuInteraction)

        let tap = UITapGestureRecognizer(target: self, action: #selector(handleTap(_:)))
        tap.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
        tap.delegate = self
        addGestureRecognizer(tap)

        let scroll = UIPanGestureRecognizer(target: self, action: #selector(handleTouchScroll(_:)))
        scroll.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
        scroll.maximumNumberOfTouches = 1
        scroll.delegate = self
        addGestureRecognizer(scroll)

        let selection = UILongPressGestureRecognizer(target: self, action: #selector(handleTouchSelection(_:)))
        selection.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
        selection.minimumPressDuration = 0.35
        selection.allowableMovement = 10
        selection.delegate = self
        addGestureRecognizer(selection)

        let pointerScroll = UIPanGestureRecognizer(target: self, action: #selector(handlePointerScroll(_:)))
        pointerScroll.allowedTouchTypes = []
        pointerScroll.allowedScrollTypesMask = .all
        pointerScroll.delegate = self
        addGestureRecognizer(pointerScroll)

        addGestureRecognizer(UIHoverGestureRecognizer(target: self, action: #selector(handleHover(_:))))
    }

    @objc private func handleTap(_ gesture: UITapGestureRecognizer) {
        guard gesture.state == .ended else { return }
        stopMomentumScrolling()
        let point = gesture.location(in: self)
        if let surface, !surface.isMouseCaptured, let link = link(at: point, on: surface) {
            (delegate as? any TerminalSurfaceOpenURLDelegate)?.terminalDidRequestOpenURL(link, kind: .unknown)
            return
        }
        if let surface, surface.hasSelection(), !surface.isMouseCaptured {
            let mods = ghostty_input_mods_e(rawValue: 0)
            surface.sendMousePos(x: point.x, y: point.y, mods: mods)
            surface.sendMouseButton(state: GHOSTTY_MOUSE_PRESS, button: GHOSTTY_MOUSE_LEFT, mods: mods)
            surface.sendMouseButton(state: GHOSTTY_MOUSE_RELEASE, button: GHOSTTY_MOUSE_LEFT, mods: mods)
        }
        if !isFirstResponder {
            becomeFirstResponder()
        }
    }

    /// A tap opens a link as a ⌘-click does. Ghostty names the link under a pointer
    /// only while the link key is down, and only when the pointer moved, so the tap
    /// takes the pointer away, asks with the key down, and takes the pointer away again.
    private func link(at point: CGPoint, on surface: TerminalSurface) -> String? {
        let none = ghostty_input_mods_e(rawValue: 0)
        surface.sendMousePos(x: -1, y: -1, mods: none)
        core.bridge.hoverLink = nil
        surface.sendMousePos(x: point.x, y: point.y, mods: TerminalInputModifiers.super_.ghosttyMods)
        let link = core.bridge.hoverLink
        surface.sendMousePos(x: -1, y: -1, mods: none)
        return link
    }

    @objc private func handleTouchSelection(_ gesture: UILongPressGestureRecognizer) {
        guard let surface else { return }
        let point = gesture.location(in: self)
        switch gesture.state {
        case .began:
            stopMomentumScrolling()
            let modifiers: TerminalInputModifiers = surface.isMouseCaptured ? .shift : []
            touchSelectionModifiers = modifiers
            surface.sendMousePos(x: point.x, y: point.y, mods: modifiers.ghosttyMods)
            for state in [GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_PRESS] {
                surface.sendMouseButton(
                    state: state,
                    button: GHOSTTY_MOUSE_LEFT,
                    mods: modifiers.ghosttyMods
                )
            }
            UISelectionFeedbackGenerator().selectionChanged()

        case .changed:
            guard let modifiers = touchSelectionModifiers else { return }
            surface.sendMousePos(x: point.x, y: point.y, mods: modifiers.ghosttyMods)

        case .ended, .cancelled, .failed:
            guard let modifiers = touchSelectionModifiers else { return }
            touchSelectionModifiers = nil
            surface.sendMousePos(x: point.x, y: point.y, mods: modifiers.ghosttyMods)
            surface.sendMouseButton(
                state: GHOSTTY_MOUSE_RELEASE,
                button: GHOSTTY_MOUSE_LEFT,
                mods: modifiers.ghosttyMods
            )
            if gesture.state == .ended, surface.hasSelection() {
                presentSelectionMenu(at: point)
            }

        default:
            break
        }
    }

    @objc private func handleTouchScroll(_ gesture: UIPanGestureRecognizer) {
        switch gesture.state {
        case .began:
            stopMomentumScrolling()

        case .changed:
            let translation = gesture.translation(in: self)
            gesture.setTranslation(.zero, in: self)
            sendScroll(translation, momentum: .none)

        case .ended:
            startMomentumScrolling(velocity: gesture.velocity(in: self))

        default:
            stopMomentumScrolling()
        }
    }

    @objc private func handlePointerScroll(_ gesture: UIPanGestureRecognizer) {
        guard gesture.state == .began || gesture.state == .changed else { return }
        if gesture.state == .began {
            stopMomentumScrolling()
        }
        let translation = gesture.translation(in: self)
        gesture.setTranslation(.zero, in: self)
        sendScroll(translation, momentum: .none)
    }

    @objc private func handleHover(_ gesture: UIHoverGestureRecognizer) {
        guard gesture.state == .began || gesture.state == .changed else { return }
        let point = gesture.location(in: self)
        surface?.sendMousePos(
            x: point.x,
            y: point.y,
            mods: TerminalInputModifiers(from: gesture.modifierFlags).ghosttyMods
        )
    }

    private func sendScroll(
        _ translation: CGPoint,
        momentum: TerminalScrollModifiers.Momentum
    ) {
        let scale = displayScale
        surface?.sendMouseScroll(
            x: translation.x * scale,
            y: translation.y * scale,
            mods: TerminalScrollModifiers(precision: true, momentum: momentum).rawValue
        )
    }

    func startMomentumScrolling(velocity: CGPoint) {
        guard hypot(velocity.x, velocity.y) > Self.minimumMomentumVelocity else { return }
        stopMomentumScrolling(sendTerminalEndEvent: false)
        momentumVelocity = velocity
        sendScroll(.zero, momentum: .began)
        let link = CADisplayLink(target: self, selector: #selector(momentumScrollFrame(_:)))
        link.add(to: .main, forMode: .common)
        momentumDisplayLink = link
    }

    @objc private func momentumScrollFrame(_ link: CADisplayLink) {
        let elapsed = CGFloat(max(link.targetTimestamp - link.timestamp, 0))
        let decay = pow(Self.scrollDecelerationRate, elapsed * 1000)
        momentumVelocity = CGPoint(
            x: momentumVelocity.x * decay,
            y: momentumVelocity.y * decay
        )
        guard hypot(momentumVelocity.x, momentumVelocity.y) > Self.minimumMomentumVelocity else {
            stopMomentumScrolling()
            return
        }
        sendScroll(
            CGPoint(x: momentumVelocity.x * elapsed, y: momentumVelocity.y * elapsed),
            momentum: .changed
        )
    }

    func stopMomentumScrolling(sendTerminalEndEvent: Bool = true) {
        guard let momentumDisplayLink else { return }
        momentumDisplayLink.invalidate()
        self.momentumDisplayLink = nil
        momentumVelocity = .zero
        if sendTerminalEndEvent {
            sendScroll(.zero, momentum: .none)
        }
    }

    override open func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard handlePointerTouches(touches, phase: .began, event: event) else {
            super.touchesBegan(touches, with: event)
            return
        }
    }

    override open func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard handlePointerTouches(touches, phase: .moved, event: event) else {
            super.touchesMoved(touches, with: event)
            return
        }
    }

    override open func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard handlePointerTouches(touches, phase: .ended, event: event) else {
            super.touchesEnded(touches, with: event)
            return
        }
    }

    override open func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard handlePointerTouches(touches, phase: .cancelled, event: event) else {
            super.touchesCancelled(touches, with: event)
            return
        }
    }

    private func handlePointerTouches(
        _ touches: Set<UITouch>,
        phase: UITouch.Phase,
        event: UIEvent?
    ) -> Bool {
        guard let touch = touches.first(where: { $0.type == .indirectPointer }) else {
            return false
        }
        guard let surface else { return true }
        let point = touch.location(in: self)
        let mods = TerminalInputModifiers(from: event?.modifierFlags ?? []).ghosttyMods
        surface.sendMousePos(x: point.x, y: point.y, mods: mods)

        switch phase {
        case .began:
            stopMomentumScrolling()
            let button = event?.buttonMask.contains(.secondary) == true
                ? GHOSTTY_MOUSE_RIGHT
                : GHOSTTY_MOUSE_LEFT
            activePointerButton = button
            if button == GHOSTTY_MOUSE_RIGHT, !surface.isMouseCaptured {
                presentSelectionMenu(at: point)
                return true
            }
            if !isFirstResponder {
                becomeFirstResponder()
            }
            surface.sendMouseButton(state: GHOSTTY_MOUSE_PRESS, button: button, mods: mods)

        case .ended, .cancelled:
            guard let button = activePointerButton else { return true }
            activePointerButton = nil
            if button == GHOSTTY_MOUSE_RIGHT, !surface.isMouseCaptured {
                return true
            }
            surface.sendMouseButton(state: GHOSTTY_MOUSE_RELEASE, button: button, mods: mods)

        default:
            break
        }
        return true
    }

    override open func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        if gestureRecognizer is UIPanGestureRecognizer,
           gestureRecognizer.allowedTouchTypes.isEmpty == false,
           touchSelectionModifiers != nil
        {
            return false
        }
        return super.gestureRecognizerShouldBegin(gestureRecognizer)
    }

    open func editMenuInteraction(
        _: UIEditMenuInteraction,
        menuFor _: UIEditMenuConfiguration,
        suggestedActions _: [UIMenuElement]
    ) -> UIMenu? {
        let elements = selectionMenuElements()
        return elements.isEmpty ? nil : UIMenu(children: elements)
    }
}
#endif
