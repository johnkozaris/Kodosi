@testable import GhosttyTerminal
import Testing

struct TerminalKeyStrokeTests {
    @Test
    func `typed letters carry their physical key and unshifted codepoint`() {
        #expect(TerminalKeyStroke.typed(Character("a")) == TerminalKeyStroke(
            keyCode: 0x00,
            modifiers: [],
            consumedModifiers: [],
            text: "a",
            unshiftedCodepoint: 97
        ))
        #expect(TerminalKeyStroke.typed(Character("A")) == TerminalKeyStroke(
            keyCode: 0x00,
            modifiers: .shift,
            consumedModifiers: .shift,
            text: "A",
            unshiftedCodepoint: 97
        ))
        #expect(TerminalKeyStroke.typed(Character("!")) == TerminalKeyStroke(
            keyCode: 0x12,
            modifiers: .shift,
            consumedModifiers: .shift,
            text: "!",
            unshiftedCodepoint: 49
        ))
    }

    @Test
    func `typed line breaks and tabs become editing keys without text`() {
        for newline in ["\n", "\r", "\r\n"] {
            let stroke = TerminalKeyStroke.typed(Character(newline))
            #expect(stroke.keyCode == 0x24)
            #expect(stroke.text == nil)
            #expect(stroke.modifiers.isEmpty)
        }
        let tab = TerminalKeyStroke.typed(Character("\t"))
        #expect(tab.keyCode == 0x30)
        #expect(tab.text == nil)
    }

    @Test
    func `typed text outside the US layout keeps its text without a physical key`() {
        #expect(TerminalKeyStroke.typed(Character("é")) == TerminalKeyStroke(
            keyCode: TerminalHardwareKeyMap.unidentifiedKeyCode,
            modifiers: [],
            consumedModifiers: [],
            text: "é",
            unshiftedCodepoint: 0xE9
        ))
        let emoji = TerminalKeyStroke.typed(Character("👍🏽"))
        #expect(emoji.keyCode == TerminalHardwareKeyMap.unidentifiedKeyCode)
        #expect(emoji.text == "👍🏽")
        #expect(emoji.unshiftedCodepoint == 0)
        #expect(TerminalKeyStroke.typed("ls -la\n").count == 7)
    }

    @Test
    func `synthetic keys only carry text when no shortcut modifier is held`() {
        #expect(TerminalKeyStroke.key(hidUsage: 0x06, modifiers: .ctrl) == TerminalKeyStroke(
            keyCode: 0x08,
            modifiers: .ctrl,
            consumedModifiers: [],
            text: nil,
            unshiftedCodepoint: 99
        ))
        #expect(TerminalKeyStroke.key(hidUsage: 0x05, modifiers: .alt) == TerminalKeyStroke(
            keyCode: 0x0B,
            modifiers: .alt,
            consumedModifiers: [],
            text: "b",
            unshiftedCodepoint: 98
        ))
        #expect(TerminalKeyStroke.key(hidUsage: 0x2D, modifiers: .shift).text == "_")
        #expect(TerminalKeyStroke.key(hidUsage: 0x52) == TerminalKeyStroke(
            keyCode: 0x7E,
            modifiers: [],
            consumedModifiers: [],
            text: nil,
            unshiftedCodepoint: 0
        ))
    }
}

struct TerminalHardwareKeyMapTests {
    @Test
    func `hid usages map to distinct mac virtual key codes`() {
        let keyCodes = TerminalHardwareKeyMap.macKeyCodes.values
        #expect(Set(keyCodes).count == keyCodes.count)
        #expect(TerminalHardwareKeyMap.keyCode(forHIDUsage: 0x29) == 0x35)
        #expect(TerminalHardwareKeyMap.keyCode(forHIDUsage: 0x2A) == 0x33)
        #expect(TerminalHardwareKeyMap.keyCode(forHIDUsage: 0x4C) == 0x75)
        #expect(TerminalHardwareKeyMap.keyCode(forHIDUsage: 0x50) == 0x7B)
        #expect(TerminalHardwareKeyMap.keyCode(forHIDUsage: 0xE3) == 0x37)
    }

    @Test
    func `unknown hid usages stay outside the mac key code range`() {
        #expect(TerminalHardwareKeyMap.keyCode(forHIDUsage: 0x46) == TerminalHardwareKeyMap.unidentifiedKeyCode)
        #expect(TerminalHardwareKeyMap.unidentifiedKeyCode > 0xFF)
    }

    @Test
    func `us layout characters round trip through their keys`() {
        for usage in TerminalHardwareKeyMap.macKeyCodes.keys {
            for shifted in [false, true] {
                guard let character = TerminalHardwareKeyMap.character(
                    forHIDUsage: usage,
                    shifted: shifted
                ) else { continue }
                let key = TerminalHardwareKeyMap.hidUsage(for: character)
                #expect(key?.usage == usage)
                #expect(key?.shifted == (shifted && character != " "))
            }
        }
        #expect(TerminalHardwareKeyMap.isModifier(hidUsage: 0xE0))
        #expect(!TerminalHardwareKeyMap.isModifier(hidUsage: 0x28))
    }
}
