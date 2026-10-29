/// The DOM key, DOM code and Windows key code of a keyboard key, by its Mac key code or its
/// USB HID usage. A key that types a character has no DOM key here: the character is the key.
enum BrowserKeys {
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
        var entries = letters.enumerated().map { index, mac in
            Entry(mac, 0x04 + index, "Key\(Character(UnicodeScalar(65 + index) ?? "A"))", windows: 65 + index)
        }
        entries += digits.enumerated().map { digit, mac in
            Entry(mac, digit == 0 ? 0x27 : 0x1D + digit, "Digit\(digit)", windows: 48 + digit)
        }
        entries += functions.enumerated().map { index, mac in
            Entry(mac, 0x3A + index, "F\(index + 1)", named: "F\(index + 1)", windows: 112 + index)
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
        ]
        return entries
    }()

    private static let byMac = Dictionary(entries.map { ($0.mac, $0.key) }, uniquingKeysWith: { first, _ in first })
    private static let byUsage = Dictionary(entries.map { ($0.usage, $0.key) }, uniquingKeysWith: { first, _ in first })
}
