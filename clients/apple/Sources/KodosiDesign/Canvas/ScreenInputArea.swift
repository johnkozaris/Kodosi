import KodosiBridge
import SwiftUI

/// Takes this device's pointer, keys and gestures over a computer's screen. The pointer and keys
/// go to the computer, while pinching zooms the view.
struct ScreenInputArea {
    let feed: ScreenFeed
    let typing: Bool
    /// The local pointer is over the view and stands in for the computer's pointer.
    let inside: Binding<Bool>
}

#if os(macOS)
    import AppKit

    extension ScreenInputArea: NSViewRepresentable {
        func makeNSView(context _: Context) -> ScreenInputView {
            ScreenInputView(feed: feed)
        }

        func updateNSView(_ view: ScreenInputView, context _: Context) {
            view.feed = feed
            view.inside = { inside.wrappedValue = $0 }
            view.show(shape: feed.pointer.map(\.shape), of: feed.shapes)
        }

        static func dismantleNSView(_ view: ScreenInputView, coordinator _: ()) {
            view.stopPanning()
        }
    }

    final class ScreenInputView: NSView {
        /// The edge band of a zoomed view that moves it, and how fast at the very edge.
        private static let panBand: CGFloat = 36
        private static let fastestPan: CGFloat = 900
        /// The modifier flags of each side's modifier key, by its key code.
        private static let sides: [UInt16: UInt] = [56: 0x02, 60: 0x04, 59: 0x01, 62: 0x2000, 58: 0x20, 61: 0x40, 55: 0x08, 54: 0x10]
        private static let capsLock: UInt16 = 57

        var feed: ScreenFeed
        var inside: (Bool) -> Void = { _ in }
        private var shape: UInt64?
        private var cursors: [UInt64: NSCursor] = [:]
        private var pointer: CGPoint?
        private var held = ViewerInput.MouseButton.none
        private var panning: CADisplayLink?
        private var panned: CFTimeInterval?

        init(feed: ScreenFeed) {
            self.feed = feed
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override var isFlipped: Bool {
            true
        }

        override var acceptsFirstResponder: Bool {
            true
        }

        override func acceptsFirstMouse(for _: NSEvent?) -> Bool {
            true
        }

        /// The computer's pointer takes the shape that it has there.
        func show(shape: UInt64?, of shapes: [UInt64: ScreenShape]) {
            guard let shape, shape != self.shape, let picture = shapes[shape] else { return }
            self.shape = shape
            if cursors[shape] == nil {
                cursors[shape] = NSCursor(image: NSImage(cgImage: picture.image, size: picture.size), hotSpot: picture.hotspot)
            }
            window?.invalidateCursorRects(for: self)
            if pointer != nil {
                cursor.set()
            }
        }

        private var cursor: NSCursor {
            shape.flatMap { cursors[$0] } ?? .arrow
        }

        override func resetCursorRects() {
            addCursorRect(bounds, cursor: cursor)
        }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas {
                removeTrackingArea(area)
            }
            addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                                           owner: self))
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window == nil {
                stopPanning()
            }
        }

        override func mouseEntered(with event: NSEvent) {
            point(at: event)
            inside(true)
        }

        override func mouseExited(with _: NSEvent) {
            pointer = nil
            stopPanning()
            inside(false)
        }

        override func mouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            mouse(.mousePressed, event, button: .left)
        }

        override func mouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .left)
        }

        override func rightMouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            mouse(.mousePressed, event, button: .right)
        }

        override func rightMouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .right)
        }

        override func otherMouseDown(with event: NSEvent) {
            mouse(.mousePressed, event, button: .middle)
        }

        override func otherMouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .middle)
        }

        override func mouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .left)
        }

        override func rightMouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .right)
        }

        override func otherMouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .middle)
        }

        override func mouseMoved(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .none)
        }

        override func scrollWheel(with event: NSEvent) {
            guard let point = feed.point(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 40
            feed.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Double(-event.scrollingDeltaX * scale),
                             deltaY: Double(-event.scrollingDeltaY * scale), modifiers: ViewerInput.Modifiers(event.modifierFlags)))
        }

        override func magnify(with event: NSEvent) {
            feed.zoom(by: 1 + event.magnification, at: convert(event.locationInWindow, from: nil), in: bounds.size)
            follow()
        }

        override func smartMagnify(with event: NSEvent) {
            if feed.zoom > 1 {
                feed.showWhole()
            } else {
                feed.zoom(by: 2, at: convert(event.locationInWindow, from: nil), in: bounds.size)
            }
            follow()
        }

        /// Shortcuts of the app's menus stay with Kodosi, and the others go to the computer. macOS
        /// sends no key up for a key that went down with Command, so it goes down and up at once.
        override func performKeyEquivalent(with event: NSEvent) -> Bool {
            guard window?.firstResponder === self, event.modifierFlags.contains(.command) else { return false }
            if NSApp.mainMenu?.performKeyEquivalent(with: event) == true {
                return true
            }
            press(event)
            return true
        }

        override func keyDown(with event: NSEvent) {
            if event.modifierFlags.contains(.command) {
                press(event)
            } else {
                key(event, down: true)
            }
        }

        override func keyUp(with event: NSEvent) {
            if !event.modifierFlags.contains(.command) {
                key(event, down: false)
            }
        }

        override func flagsChanged(with event: NSEvent) {
            guard let key = ViewerKeys.key(macKeyCode: event.keyCode) else { return }
            let held = ViewerInput.Modifiers(event.modifierFlags)
            if event.keyCode == Self.capsLock {
                feed.send(.key(key, typed: nil, down: true, modifiers: held))
                feed.send(.key(key, typed: nil, down: false, modifiers: held))
            } else if let side = Self.sides[event.keyCode] {
                feed.send(.key(key, typed: nil, down: event.modifierFlags.rawValue & side != 0, modifiers: held))
            }
        }

        func stopPanning() {
            panning?.invalidate()
            panning = nil
            panned = nil
        }

        private func press(_ event: NSEvent) {
            key(event, down: true)
            key(event, down: false)
        }

        private func key(_ event: NSEvent, down: Bool) {
            guard let key = ViewerKeys.key(macKeyCode: event.keyCode) else { return }
            let held = ViewerInput.Modifiers(event.modifierFlags)
            let typed = held.isDisjoint(with: [.control, .meta]) ? event.characters : event.charactersIgnoringModifiers
            feed.send(.key(key, typed: typed?.isEmpty == false ? typed : nil, down: down, modifiers: held))
        }

        private func mouse(_ type: ViewerInput.MouseEvent, _ event: NSEvent, button: ViewerInput.MouseButton) {
            self.point(at: event)
            if type == .mousePressed {
                held = button
            } else if type == .mouseReleased {
                held = .none
            }
            guard let point = feed.point(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let clicks = type == .mouseMoved ? 0 : event.clickCount
            feed.send(.mouse(type, x: point.x, y: point.y, button: button, clicks: clicks,
                             modifiers: ViewerInput.Modifiers(event.modifierFlags)))
        }

        private func point(at event: NSEvent) {
            pointer = convert(event.locationInWindow, from: nil)
            follow()
        }

        /// A zoomed view moves while the pointer rests at its edge, as Screen Sharing's does.
        private func follow() {
            guard feed.zoom > 1, pan != .zero else {
                stopPanning()
                return
            }
            if panning == nil {
                let link = displayLink(target: self, selector: #selector(panFrame))
                link.add(to: .main, forMode: .common)
                panning = link
            }
        }

        /// How fast the view moves, in view points a second, with the pointer where it is.
        private var pan: CGSize {
            guard let pointer, bounds.width > Self.panBand * 3, bounds.height > Self.panBand * 3 else { return .zero }
            let speed = { (depth: CGFloat) in Self.fastestPan * min(max(depth / Self.panBand, 0), 1) }
            return CGSize(width: speed(Self.panBand - (bounds.width - pointer.x)) - speed(Self.panBand - pointer.x),
                          height: speed(Self.panBand - (bounds.height - pointer.y)) - speed(Self.panBand - pointer.y))
        }

        /// Stops once the view reaches the edge of the source, until the pointer moves again.
        @objc private func panFrame(_ link: CADisplayLink) {
            let pan = pan
            guard feed.zoom > 1, pan != .zero, let pointer else {
                stopPanning()
                return
            }
            guard let panned else {
                panned = link.timestamp
                return
            }
            self.panned = link.timestamp
            let elapsed = link.timestamp - panned
            let before = feed.placement(in: bounds.size)?.look
            feed.pan(by: CGSize(width: pan.width * elapsed, height: pan.height * elapsed), in: bounds.size)
            guard feed.placement(in: bounds.size)?.look != before else {
                stopPanning()
                return
            }
            if let point = feed.point(pointer, in: bounds.size) {
                feed.send(.mouse(.mouseMoved, x: point.x, y: point.y, button: held))
            }
        }
    }
#else
    import UIKit

    extension ScreenInputArea: UIViewRepresentable {
        func makeUIView(context _: Context) -> ScreenTouchView {
            ScreenTouchView(feed: feed)
        }

        func updateUIView(_ view: ScreenTouchView, context _: Context) {
            view.feed = feed
            view.target = feed
            view.wantsKeyboard(typing)
        }
    }

    /// One finger points, clicks and scrolls on the computer, as in the shared browser. Two fingers
    /// pinch to zoom the view and move it while it is zoomed.
    final class ScreenTouchView: TouchInputView, UIGestureRecognizerDelegate {
        var feed: ScreenFeed
        private let pinch = UIPinchGestureRecognizer()
        private let move = UIPanGestureRecognizer()

        init(feed: ScreenFeed) {
            self.feed = feed
            super.init(target: feed)
            scroll.maximumNumberOfTouches = 1
            pinch.addTarget(self, action: #selector(pinched))
            move.addTarget(self, action: #selector(moved))
            move.minimumNumberOfTouches = 2
            for gesture in [pinch, move] {
                gesture.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
                gesture.delegate = self
                addGestureRecognizer(gesture)
            }
        }

        @objc private func pinched(_ gesture: UIPinchGestureRecognizer) {
            guard gesture.state == .began || gesture.state == .changed else { return }
            feed.zoom(by: gesture.scale, at: gesture.location(in: self), in: bounds.size)
            gesture.scale = 1
        }

        @objc private func moved(_ gesture: UIPanGestureRecognizer) {
            guard gesture.state == .began || gesture.state == .changed else { return }
            let moved = gesture.translation(in: self)
            feed.pan(by: CGSize(width: -moved.x, height: -moved.y), in: bounds.size)
            gesture.setTranslation(.zero, in: self)
        }

        func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            [pinch, move].contains(gesture) && [pinch, move].contains(other)
        }
    }
#endif
