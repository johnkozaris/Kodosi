import KodosiBridge
import SwiftUI
#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// A stream that a view sends its pointer, keys and text to: the shared browser, or the screen of
/// a computer.
@MainActor
protocol InputTarget: AnyObject {
    /// The stream's point under a view point, or `nil` outside the stream.
    func streamPoint(_ point: CGPoint, in size: CGSize) -> ViewerInput.Point?
    /// View points per stream point, so that a scroll follows the finger.
    func fit(in size: CGSize) -> CGFloat?
    func send(_ input: ViewerInput)
}

extension BrowserFeed: InputTarget {
    func streamPoint(_ point: CGPoint, in size: CGSize) -> ViewerInput.Point? {
        browserPoint(point, in: size)
    }

    func fit(in size: CGSize) -> CGFloat? {
        let page = window.size
        guard page.width > 0, page.height > 0, size.width > 0, size.height > 0 else { return nil }
        return min(size.width / page.width, size.height / page.height)
    }
}

extension ScreenFeed: InputTarget {
    func streamPoint(_ point: CGPoint, in size: CGSize) -> ViewerInput.Point? {
        self.point(point, in: size)
    }

    func fit(in size: CGSize) -> CGFloat? {
        placement(in: size)?.fit(in: size)
    }
}

extension InputTarget {
    func type(_ text: String) {
        for character in text {
            if let key = ViewerKeys.key(character: character) {
                send(.key(.keyDown, key: key.named ?? String(character), code: key.code, text: String(character), keyCode: key.windows))
                send(.key(.keyUp, key: key.named ?? String(character), code: key.code, keyCode: key.windows))
            } else {
                send(.text(String(character)))
            }
        }
    }

    func press(_ key: ViewerKeys.Key, typed: String?, down: Bool, modifiers: ViewerInput.Modifiers) {
        send(.key(key, typed: typed, down: down, modifiers: modifiers))
    }
}

extension ViewerKeys {
    /// Maps ASCII characters to US-layout key codes for software keyboards, which only provide
    /// text and no physical key codes.
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
    extension ViewerInput.Modifiers {
        init(_ flags: NSEvent.ModifierFlags) {
            self = []
            if flags.contains(.option) {
                insert(.alt)
            }
            if flags.contains(.control) {
                insert(.control)
            }
            if flags.contains(.command) {
                insert(.meta)
            }
            if flags.contains(.shift) {
                insert(.shift)
            }
        }
    }
#else
    extension ViewerInput.Modifiers {
        init(_ flags: UIKeyModifierFlags) {
            self = []
            if flags.contains(.alternate) {
                insert(.alt)
            }
            if flags.contains(.control) {
                insert(.control)
            }
            if flags.contains(.command) {
                insert(.meta)
            }
            if flags.contains(.shift) {
                insert(.shift)
            }
        }
    }
#endif
