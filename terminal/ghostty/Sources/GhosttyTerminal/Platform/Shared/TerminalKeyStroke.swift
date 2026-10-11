struct TerminalKeyStroke: Equatable {
    var keyCode: UInt32
    var modifiers: TerminalInputModifiers
    var consumedModifiers: TerminalInputModifiers
    var text: String?
    var unshiftedCodepoint: UInt32

    static func typed(_ text: String) -> [TerminalKeyStroke] {
        text.map(typed)
    }

    static func typed(_ character: Character) -> TerminalKeyStroke {
        switch character {
        case "\n", "\r", "\r\n":
            return key(hidUsage: 0x28)
        case "\t":
            return key(hidUsage: 0x2B)
        default:
            break
        }
        guard let layout = TerminalHardwareKeyMap.hidUsage(for: character) else {
            let scalars = character.unicodeScalars
            return TerminalKeyStroke(
                keyCode: TerminalHardwareKeyMap.unidentifiedKeyCode,
                modifiers: [],
                consumedModifiers: [],
                text: String(character),
                unshiftedCodepoint: scalars.count == 1 ? scalars.first?.value ?? 0 : 0
            )
        }
        let modifiers: TerminalInputModifiers = layout.shifted ? .shift : []
        return TerminalKeyStroke(
            keyCode: TerminalHardwareKeyMap.keyCode(forHIDUsage: layout.usage),
            modifiers: modifiers,
            consumedModifiers: modifiers,
            text: String(character),
            unshiftedCodepoint: unshiftedCodepoint(forHIDUsage: layout.usage)
        )
    }

    static func key(
        hidUsage usage: UInt16,
        modifiers: TerminalInputModifiers = []
    ) -> TerminalKeyStroke {
        let producesText = modifiers.isDisjoint(with: [.ctrl, .super_])
        let character = TerminalHardwareKeyMap.character(
            forHIDUsage: usage,
            shifted: modifiers.contains(.shift)
        )
        let text = producesText ? character.map(String.init) : nil
        return TerminalKeyStroke(
            keyCode: TerminalHardwareKeyMap.keyCode(forHIDUsage: usage),
            modifiers: modifiers,
            consumedModifiers: text == nil ? [] : modifiers.intersection(.shift),
            text: text,
            unshiftedCodepoint: unshiftedCodepoint(forHIDUsage: usage)
        )
    }

    private static func unshiftedCodepoint(forHIDUsage usage: UInt16) -> UInt32 {
        TerminalHardwareKeyMap.character(forHIDUsage: usage, shifted: false)?
            .unicodeScalars.first?.value ?? 0
    }
}
