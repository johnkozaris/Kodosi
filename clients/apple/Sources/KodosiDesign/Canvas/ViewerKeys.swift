import KodosiBridge

/// The physical keys of a keyboard by their DOM `code`, which the shared browser and the screen of
/// a computer read. Character-producing keys leave `named` nil so callers use the actual typed
/// character as the DOM `key` value.
enum ViewerKeys {
    struct Key {
        let code: String
        let named: String?
        let windows: Int
    }

    private struct Entry {
        let mac: UInt16
        let usage: Int
        let key: Key

        init(_ mac: UInt16, _ usage: Int, _ code: String, named: String? = nil, windows: Int) {
            self.mac = mac
            self.usage = usage
            key = Key(code: code, named: named, windows: windows)
        }
    }

    static func key(macKeyCode: UInt16) -> Key? {
        byMac[macKeyCode]
    }

    static func key(hidUsage: Int) -> Key? {
        byUsage[hidUsage]
    }

    private static let entries: [Entry] = {
        let letters: [UInt16] = [0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16, 6]
        let digits: [UInt16] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25]
        let functions: [UInt16] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111]
        let keypad: [UInt16] = [82, 83, 84, 85, 86, 87, 88, 89, 91, 92]
        var entries = letters.enumerated().map { index, mac in
            Entry(mac, 0x04 + index, "Key\(Character(UnicodeScalar(65 + index) ?? "A"))", windows: 65 + index)
        }
        entries += digits.enumerated().map { digit, mac in
            Entry(mac, digit == 0 ? 0x27 : 0x1D + digit, "Digit\(digit)", windows: 48 + digit)
        }
        entries += functions.enumerated().map { index, mac in
            Entry(mac, 0x3A + index, "F\(index + 1)", named: "F\(index + 1)", windows: 112 + index)
        }
        entries += keypad.enumerated().map { digit, mac in
            Entry(mac, digit == 0 ? 0x62 : 0x58 + digit, "Numpad\(digit)", windows: 96 + digit)
        }
        entries += [
            Entry(27, 0x2D, "Minus", windows: 189), Entry(24, 0x2E, "Equal", windows: 187),
            Entry(33, 0x2F, "BracketLeft", windows: 219), Entry(30, 0x30, "BracketRight", windows: 221),
            Entry(42, 0x31, "Backslash", windows: 220), Entry(41, 0x33, "Semicolon", windows: 186),
            Entry(39, 0x34, "Quote", windows: 222), Entry(50, 0x35, "Backquote", windows: 192),
            Entry(43, 0x36, "Comma", windows: 188), Entry(47, 0x37, "Period", windows: 190), Entry(44, 0x38, "Slash", windows: 191),
            Entry(36, 0x28, "Enter", named: "Enter", windows: 13), Entry(76, 0x58, "NumpadEnter", named: "Enter", windows: 13),
            Entry(53, 0x29, "Escape", named: "Escape", windows: 27), Entry(51, 0x2A, "Backspace", named: "Backspace", windows: 8),
            Entry(48, 0x2B, "Tab", named: "Tab", windows: 9), Entry(49, 0x2C, "Space", named: " ", windows: 32),
            Entry(115, 0x4A, "Home", named: "Home", windows: 36), Entry(116, 0x4B, "PageUp", named: "PageUp", windows: 33),
            Entry(117, 0x4C, "Delete", named: "Delete", windows: 46), Entry(119, 0x4D, "End", named: "End", windows: 35),
            Entry(121, 0x4E, "PageDown", named: "PageDown", windows: 34), Entry(124, 0x4F, "ArrowRight", named: "ArrowRight", windows: 39),
            Entry(123, 0x50, "ArrowLeft", named: "ArrowLeft", windows: 37), Entry(125, 0x51, "ArrowDown", named: "ArrowDown", windows: 40),
            Entry(126, 0x52, "ArrowUp", named: "ArrowUp", windows: 38),
            Entry(65, 0x63, "NumpadDecimal", windows: 110), Entry(67, 0x55, "NumpadMultiply", windows: 106),
            Entry(69, 0x57, "NumpadAdd", windows: 107), Entry(75, 0x54, "NumpadDivide", windows: 111),
            Entry(78, 0x56, "NumpadSubtract", windows: 109), Entry(81, 0x67, "NumpadEqual", windows: 187),
            Entry(71, 0x53, "NumLock", named: "Clear", windows: 12),
            Entry(56, 0xE1, "ShiftLeft", named: "Shift", windows: 16), Entry(60, 0xE5, "ShiftRight", named: "Shift", windows: 16),
            Entry(59, 0xE0, "ControlLeft", named: "Control", windows: 17),
            Entry(62, 0xE4, "ControlRight", named: "Control", windows: 17),
            Entry(58, 0xE2, "AltLeft", named: "Alt", windows: 18), Entry(61, 0xE6, "AltRight", named: "Alt", windows: 18),
            Entry(55, 0xE3, "MetaLeft", named: "Meta", windows: 91), Entry(54, 0xE7, "MetaRight", named: "Meta", windows: 92),
            Entry(57, 0x39, "CapsLock", named: "CapsLock", windows: 20),
        ]
        return entries
    }()

    private static let byMac = Dictionary(entries.map { ($0.mac, $0.key) }, uniquingKeysWith: { first, _ in first })
    private static let byUsage = Dictionary(entries.map { ($0.usage, $0.key) }, uniquingKeysWith: { first, _ in first })
}

extension ViewerInput {
    /// A key that goes down or up, with the text that it types going down.
    static func key(_ key: ViewerKeys.Key, typed: String?, down: Bool, modifiers: Modifiers) -> ViewerInput {
        let name = key.named ?? typed ?? ""
        let text = down ? (key.code == "Space" ? " " : key.named == "Enter" ? "\r" : key.named == nil ? typed : nil) : nil
        return .key(down ? .keyDown : .keyUp, key: name, code: key.code, text: text, keyCode: key.windows, modifiers: modifiers)
    }
}
