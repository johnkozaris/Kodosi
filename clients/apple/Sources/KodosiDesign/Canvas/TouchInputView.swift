#if os(iOS)
    import KodosiBridge
    import UIKit

    /// Touches as a pointer on a stream: a tap clicks, a long press clicks with the right button or
    /// drags, a pan scrolls with momentum, and two fingers tap with the right button. A trackpad or
    /// a mouse points, and a keyboard types.
    class TouchInputView: UIView, UIKeyInput {
        private static let dragDistance: CGFloat = 10
        private static let slowestMomentum: CGFloat = 20

        var target: any InputTarget
        let scroll: UIPanGestureRecognizer
        private var keyboard = false
        private var taps = BrowserTaps()
        private var holdStart: CGPoint?
        private var dragging = false
        private var pointerButton: ViewerInput.MouseButton?
        private var momentum: CADisplayLink?
        private var momentumVelocity = CGPoint.zero
        private var momentumPoint = CGPoint.zero
        private var touchStoppedMomentum = false

        init(target: any InputTarget) {
            self.target = target
            scroll = UIPanGestureRecognizer()
            super.init(frame: .zero)
            backgroundColor = .clear
            let hold = UILongPressGestureRecognizer(target: self, action: #selector(held))
            let tap = UITapGestureRecognizer(target: self, action: #selector(tapped))
            tap.require(toFail: hold)
            let pair = UITapGestureRecognizer(target: self, action: #selector(pairTapped))
            pair.numberOfTouchesRequired = 2
            scroll.addTarget(self, action: #selector(scrolled))
            for touch in [hold, tap, pair, scroll] {
                touch.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
                addGestureRecognizer(touch)
            }
            let pointerScroll = UIPanGestureRecognizer(target: self, action: #selector(pointerScrolled))
            pointerScroll.allowedTouchTypes = []
            pointerScroll.allowedScrollTypesMask = .all
            addGestureRecognizer(pointerScroll)
            addGestureRecognizer(UIHoverGestureRecognizer(target: self, action: #selector(hovered)))
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        /// Defers first-responder changes past the current SwiftUI update and only applies them
        /// when `wanted` changes, avoiding reentrant updates that steal focus from the address bar.
        func wantsKeyboard(_ wanted: Bool) {
            guard wanted != keyboard else { return }
            keyboard = wanted
            followKeyboard()
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if window == nil {
                stopMomentum()
            }
            followKeyboard()
        }

        private func followKeyboard() {
            Task { [weak self] in
                await Task.yield()
                guard let self, window != nil else { return }
                if keyboard, !isFirstResponder {
                    becomeFirstResponder()
                } else if !keyboard, isFirstResponder {
                    resignFirstResponder()
                }
            }
        }

        override var canBecomeFirstResponder: Bool {
            true
        }

        var hasText: Bool {
            true
        }

        var autocorrectionType = UITextAutocorrectionType.no
        var autocapitalizationType = UITextAutocapitalizationType.none

        func insertText(_ text: String) {
            if text == "\n", let enter = ViewerKeys.key(hidUsage: 0x28) {
                target.press(enter, typed: nil, down: true, modifiers: [])
                target.press(enter, typed: nil, down: false, modifiers: [])
            } else {
                target.type(text)
            }
        }

        func deleteBackward() {
            guard let backspace = ViewerKeys.key(hidUsage: 0x2A) else { return }
            target.press(backspace, typed: nil, down: true, modifiers: [])
            target.press(backspace, typed: nil, down: false, modifiers: [])
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if !handle(presses, down: true) {
                super.pressesBegan(presses, with: event)
            }
        }

        override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if !handle(presses, down: false) {
                super.pressesEnded(presses, with: event)
            }
        }

        override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if !handle(presses, down: false) {
                super.pressesCancelled(presses, with: event)
            }
        }

        private func handle(_ presses: Set<UIPress>, down: Bool) -> Bool {
            var handled = false
            for press in presses {
                guard let pressed = press.key, let key = ViewerKeys.key(hidUsage: pressed.keyCode.rawValue) else { continue }
                let held = ViewerInput.Modifiers(pressed.modifierFlags)
                let typed = held.isDisjoint(with: [.control, .meta]) ? pressed.characters : pressed.charactersIgnoringModifiers
                target.press(key, typed: typed.isEmpty ? nil : typed, down: down, modifiers: held)
                handled = true
            }
            return handled
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
            touchStoppedMomentum = momentum != nil
            stopMomentum()
            if !pointer(touches, .mousePressed, event) {
                super.touchesBegan(touches, with: event)
            }
        }

        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
            if !pointer(touches, .mouseMoved, event) {
                super.touchesMoved(touches, with: event)
            }
        }

        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
            if !pointer(touches, .mouseReleased, event) {
                super.touchesEnded(touches, with: event)
            }
        }

        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
            if !pointer(touches, .mouseReleased, event) {
                super.touchesCancelled(touches, with: event)
            }
        }

        private func pointer(_ touches: Set<UITouch>, _ type: ViewerInput.MouseEvent, _ event: UIEvent?) -> Bool {
            guard let touch = touches.first(where: { $0.type == .indirectPointer }) else { return false }
            if type == .mousePressed {
                pointerButton = event?.buttonMask.contains(.secondary) == true ? .right : .left
            }
            guard let button = pointerButton else { return true }
            if type == .mouseReleased {
                pointerButton = nil
            }
            mouse(type, at: touch.location(in: self), button: button, clicks: type == .mouseMoved ? 0 : touch.tapCount,
                  modifiers: ViewerInput.Modifiers(event?.modifierFlags ?? []))
            return true
        }

        @objc private func tapped(_ gesture: UITapGestureRecognizer) {
            guard !touchStoppedMomentum else { return }
            let location = gesture.location(in: self)
            click(at: location, button: .left, clicks: taps.click(at: location, time: CACurrentMediaTime()))
        }

        @objc private func pairTapped(_ gesture: UITapGestureRecognizer) {
            taps.forget()
            click(at: gesture.location(in: self), button: .right)
        }

        /// Long-press then drag simulates a left-button drag (for text selection); releasing
        /// without dragging simulates a right-click to open the page context menu.
        @objc private func held(_ gesture: UILongPressGestureRecognizer) {
            let location = gesture.location(in: self)
            switch gesture.state {
            case .began:
                taps.forget()
                holdStart = location
                UISelectionFeedbackGenerator().selectionChanged()
            case .changed:
                guard let start = holdStart else { return }
                if !dragging {
                    guard hypot(location.x - start.x, location.y - start.y) > Self.dragDistance else { return }
                    guard mouse(.mousePressed, at: start, button: .left, clicks: 1) else {
                        holdStart = nil
                        return
                    }
                    dragging = true
                }
                mouse(.mouseMoved, at: location, button: .left)
            case .ended, .cancelled:
                if dragging {
                    mouse(.mouseReleased, at: location, button: .left, clicks: 1)
                } else if gesture.state == .ended, let start = holdStart {
                    click(at: start, button: .right)
                }
                holdStart = nil
                dragging = false
            default:
                break
            }
        }

        @objc private func scrolled(_ gesture: UIPanGestureRecognizer) {
            guard [.began, .changed, .ended].contains(gesture.state) else { return }
            let location = gesture.location(in: self)
            scroll(by: gesture.translation(in: self), at: location)
            gesture.setTranslation(.zero, in: self)
            if gesture.state == .ended {
                startMomentum(velocity: gesture.velocity(in: self), at: location)
            }
        }

        @objc private func pointerScrolled(_ gesture: UIPanGestureRecognizer) {
            guard gesture.state == .began || gesture.state == .changed else { return }
            stopMomentum()
            scroll(by: gesture.translation(in: self), at: gesture.location(in: self))
            gesture.setTranslation(.zero, in: self)
        }

        @objc private func hovered(_ gesture: UIHoverGestureRecognizer) {
            guard gesture.state == .began || gesture.state == .changed else { return }
            mouse(.mouseMoved, at: gesture.location(in: self), button: .none, modifiers: ViewerInput.Modifiers(gesture.modifierFlags))
        }

        /// Converts view-point pan deltas to stream points so scrolling tracks the finger 1:1.
        private func scroll(by moved: CGPoint, at location: CGPoint) {
            guard moved != .zero, let fit = target.fit(in: bounds.size), fit > 0,
                  let point = target.streamPoint(location, in: bounds.size) else { return }
            target.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Double(-moved.x / fit), deltaY: Double(-moved.y / fit)))
        }

        private func startMomentum(velocity: CGPoint, at location: CGPoint) {
            guard hypot(velocity.x, velocity.y) > Self.slowestMomentum else { return }
            momentumVelocity = velocity
            momentumPoint = location
            let link = CADisplayLink(target: self, selector: #selector(momentumFrame))
            link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60)
            link.add(to: .main, forMode: .common)
            momentum = link
        }

        @objc private func momentumFrame(_ link: CADisplayLink) {
            let elapsed = CGFloat(max(link.targetTimestamp - link.timestamp, 0))
            let decay = pow(UIScrollView.DecelerationRate.normal.rawValue, elapsed * 1000)
            momentumVelocity = CGPoint(x: momentumVelocity.x * decay, y: momentumVelocity.y * decay)
            guard hypot(momentumVelocity.x, momentumVelocity.y) > Self.slowestMomentum else {
                stopMomentum()
                return
            }
            scroll(by: CGPoint(x: momentumVelocity.x * elapsed, y: momentumVelocity.y * elapsed), at: momentumPoint)
        }

        private func stopMomentum() {
            momentum?.invalidate()
            momentum = nil
        }

        private func click(at location: CGPoint, button: ViewerInput.MouseButton, clicks: Int = 1) {
            guard mouse(.mousePressed, at: location, button: button, clicks: clicks) else { return }
            mouse(.mouseReleased, at: location, button: button, clicks: clicks)
        }

        @discardableResult
        private func mouse(_ type: ViewerInput.MouseEvent, at location: CGPoint, button: ViewerInput.MouseButton,
                           clicks: Int = 0, modifiers: ViewerInput.Modifiers = []) -> Bool
        {
            guard let point = target.streamPoint(location, in: bounds.size) else { return false }
            target.send(.mouse(type, x: point.x, y: point.y, button: button, clicks: clicks, modifiers: modifiers))
            return true
        }
    }
#endif
