import KodosiBridge
import SwiftUI

/// Takes a person's pointer, keys and text for the shared browser, over the picture that
/// fills the view scaled to fit.
struct BrowserInputArea {
    let feed: BrowserFeed
    let typing: Bool
}

private extension BrowserFeed {
    func type(_ text: String) {
        for character in text {
            if let key = BrowserKeys.key(character: character) {
                send(.key(.keyDown, key: key.named ?? String(character), code: key.code, text: String(character), keyCode: key.windows))
                send(.key(.keyUp, key: key.named ?? String(character), code: key.code, keyCode: key.windows))
            } else {
                send(.text(String(character)))
            }
        }
    }

    func press(_ key: BrowserKeys.Key, typed: String?, down: Bool, modifiers: BrowserInput.Modifiers) {
        let name = key.named ?? typed ?? ""
        let text = down ? (key.code == "Space" ? " " : key.named == "Enter" ? "\r" : key.named == nil ? typed : nil) : nil
        let event: BrowserInput.KeyEvent = down ? (text == nil ? .rawKeyDown : .keyDown) : .keyUp
        send(.key(event, key: name, code: key.code, text: text, keyCode: key.windows, modifiers: modifiers))
    }
}

extension BrowserKeys {
    /// The key that types this character on a US keyboard, for text that an on-screen keyboard
    /// types.
    static func key(character: Character) -> Key? {
        guard let ascii = character.asciiValue else { return nil }
        switch character {
        case "a" ... "z", "A" ... "Z":
            let upper = Character(character.uppercased())
            return Key(code: "Key\(upper)", named: nil, windows: Int(upper.asciiValue ?? ascii))
        case "0" ... "9":
            return Key(code: "Digit\(character)", named: nil, windows: Int(ascii))
        case " ":
            return Key(code: "Space", named: " ", windows: 32)
        default:
            return nil
        }
    }
}

#if os(macOS)
    import AppKit

    extension BrowserInputArea: NSViewRepresentable {
        func makeNSView(context _: Context) -> BrowserInputView {
            BrowserInputView(feed: feed)
        }

        func updateNSView(_ view: BrowserInputView, context _: Context) {
            view.feed = feed
        }
    }

    final class BrowserInputView: NSView {
        var feed: BrowserFeed

        init(feed: BrowserFeed) {
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

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas {
                removeTrackingArea(area)
            }
            addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self))
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
            guard let point = feed.browserPoint(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 40
            feed.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Int(-event.scrollingDeltaX * scale),
                             deltaY: Int(-event.scrollingDeltaY * scale), modifiers: Self.modifiers(event.modifierFlags)))
        }

        override func keyDown(with event: NSEvent) {
            key(event, down: true)
        }

        override func keyUp(with event: NSEvent) {
            key(event, down: false)
        }

        @objc func paste(_: Any?) {
            if let text = NSPasteboard.general.string(forType: .string), !text.isEmpty {
                feed.send(.text(text))
            }
        }

        override func selectAll(_: Any?) {
            guard let key = BrowserKeys.key(macKeyCode: 0) else { return }
            feed.press(key, typed: "a", down: true, modifiers: .meta)
            feed.press(key, typed: "a", down: false, modifiers: .meta)
        }

        private func mouse(_ type: BrowserInput.MouseEvent, _ event: NSEvent, button: BrowserInput.MouseButton) {
            guard let point = feed.browserPoint(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let clicks = type == .mouseMoved ? 0 : event.clickCount
            feed.send(.mouse(type, x: point.x, y: point.y, button: button, clicks: clicks, modifiers: Self.modifiers(event.modifierFlags)))
        }

        private func key(_ event: NSEvent, down: Bool) {
            guard let key = BrowserKeys.key(macKeyCode: event.keyCode) else { return }
            let held = Self.modifiers(event.modifierFlags)
            let typed = held.isDisjoint(with: [.control, .meta]) ? event.characters : event.charactersIgnoringModifiers
            feed.press(key, typed: typed?.isEmpty == false ? typed : nil, down: down, modifiers: held)
        }

        static func modifiers(_ flags: NSEvent.ModifierFlags) -> BrowserInput.Modifiers {
            var held: BrowserInput.Modifiers = []
            if flags.contains(.option) {
                held.insert(.alt)
            }
            if flags.contains(.control) {
                held.insert(.control)
            }
            if flags.contains(.command) {
                held.insert(.meta)
            }
            if flags.contains(.shift) {
                held.insert(.shift)
            }
            return held
        }
    }

#else
    import UIKit

    extension BrowserInputArea: UIViewRepresentable {
        func makeUIView(context _: Context) -> BrowserInputView {
            BrowserInputView(feed: feed)
        }

        func updateUIView(_ view: BrowserInputView, context _: Context) {
            view.feed = feed
            if typing, !view.isFirstResponder {
                view.becomeFirstResponder()
            } else if !typing, view.isFirstResponder {
                view.resignFirstResponder()
            }
        }
    }

    final class BrowserInputView: UIView, UIKeyInput {
        var feed: BrowserFeed
        private var panned = CGPoint.zero

        init(feed: BrowserFeed) {
            self.feed = feed
            super.init(frame: .zero)
            backgroundColor = .clear
            addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped)))
            let pan = UIPanGestureRecognizer(target: self, action: #selector(panning))
            pan.allowedScrollTypesMask = .all
            addGestureRecognizer(pan)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
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
            if text == "\n", let enter = BrowserKeys.key(hidUsage: 0x28) {
                feed.press(enter, typed: nil, down: true, modifiers: [])
                feed.press(enter, typed: nil, down: false, modifiers: [])
            } else {
                feed.type(text)
            }
        }

        func deleteBackward() {
            guard let backspace = BrowserKeys.key(hidUsage: 0x2A) else { return }
            feed.press(backspace, typed: nil, down: true, modifiers: [])
            feed.press(backspace, typed: nil, down: false, modifiers: [])
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

        private func handle(_ presses: Set<UIPress>, down: Bool) -> Bool {
            var handled = false
            for press in presses {
                guard let pressed = press.key, let key = BrowserKeys.key(hidUsage: pressed.keyCode.rawValue) else { continue }
                let held = Self.modifiers(pressed.modifierFlags)
                let typed = held.isDisjoint(with: [.control, .meta]) ? pressed.characters : pressed.charactersIgnoringModifiers
                feed.press(key, typed: typed.isEmpty ? nil : typed, down: down, modifiers: held)
                handled = true
            }
            return handled
        }

        @objc private func tapped(_ gesture: UITapGestureRecognizer) {
            guard let point = feed.browserPoint(gesture.location(in: self), in: bounds.size) else { return }
            feed.send(.mouse(.mousePressed, x: point.x, y: point.y, button: .left, clicks: 1))
            feed.send(.mouse(.mouseReleased, x: point.x, y: point.y, button: .left, clicks: 1))
        }

        @objc private func panning(_ gesture: UIPanGestureRecognizer) {
            if gesture.state == .began {
                panned = .zero
            }
            let moved = gesture.translation(in: self)
            let delta = CGPoint(x: moved.x - panned.x, y: moved.y - panned.y)
            panned = moved
            guard let point = feed.browserPoint(gesture.location(in: self), in: bounds.size) else { return }
            feed.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Int(-delta.x), deltaY: Int(-delta.y)))
        }

        static func modifiers(_ flags: UIKeyModifierFlags) -> BrowserInput.Modifiers {
            var held: BrowserInput.Modifiers = []
            if flags.contains(.alternate) {
                held.insert(.alt)
            }
            if flags.contains(.control) {
                held.insert(.control)
            }
            if flags.contains(.command) {
                held.insert(.meta)
            }
            if flags.contains(.shift) {
                held.insert(.shift)
            }
            return held
        }
    }
#endif
