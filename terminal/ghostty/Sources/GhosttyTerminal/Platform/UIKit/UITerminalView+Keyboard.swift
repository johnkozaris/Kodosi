#if os(iOS)
import GhosttyKit
import UIKit

extension UITerminalView {
    private static let keyRepeatDelay: TimeInterval = 0.4
    private static let keyRepeatInterval: TimeInterval = 0.05
    private static let textSystemKeys: Set<UInt16> = [0x28, 0x2A, 0x2B, 0x58]

    override open func pressesBegan(
        _ presses: Set<UIPress>,
        with event: UIPressesEvent?
    ) {
        var unhandled: Set<UIPress> = []
        for press in presses {
            guard let key = press.key, handleKeyDown(key) else {
                unhandled.insert(press)
                continue
            }
        }
        if !unhandled.isEmpty {
            super.pressesBegan(unhandled, with: event)
        }
    }

    override open func pressesEnded(
        _ presses: Set<UIPress>,
        with event: UIPressesEvent?
    ) {
        let unhandled = releaseKeys(presses)
        hardwareKeyHandled = false
        if !unhandled.isEmpty {
            super.pressesEnded(unhandled, with: event)
        }
    }

    override open func pressesCancelled(
        _ presses: Set<UIPress>,
        with event: UIPressesEvent?
    ) {
        let unhandled = releaseKeys(presses)
        hardwareKeyHandled = false
        if !unhandled.isEmpty {
            super.pressesCancelled(unhandled, with: event)
        }
    }

    public func sendKeyPress(
        _ usage: UIKeyboardHIDUsage,
        modifiers: UIKeyModifierFlags = []
    ) {
        if inputHandler.hasMarkedText {
            inputHandler.unmarkText()
        }
        let stroke = TerminalKeyStroke.key(
            hidUsage: UInt16(truncatingIfNeeded: usage.rawValue),
            modifiers: TerminalInputModifiers(from: modifiers)
        )
        send(stroke, action: GHOSTTY_ACTION_PRESS)
        send(stroke, action: GHOSTTY_ACTION_RELEASE)
    }

    @discardableResult
    func send(
        _ stroke: TerminalKeyStroke,
        action: ghostty_input_action_e,
        composing: Bool = false
    ) -> Bool {
        guard let surface else { return false }
        var event = ghostty_input_key_s()
        event.action = action
        event.keycode = stroke.keyCode
        event.mods = stroke.modifiers.ghosttyMods
        event.consumed_mods = stroke.consumedModifiers.ghosttyMods
        event.unshifted_codepoint = stroke.unshiftedCodepoint
        event.composing = composing
        guard action != GHOSTTY_ACTION_RELEASE,
              let text = stroke.text,
              !text.isEmpty
        else {
            return surface.sendKeyEvent(event)
        }
        return text.withCString { pointer in
            event.text = pointer
            return surface.sendKeyEvent(event)
        }
    }

    func releasePressedKeys() {
        stopKeyRepeat()
        for usage in pressedKeyUsages {
            send(
                TerminalKeyStroke(
                    keyCode: TerminalHardwareKeyMap.keyCode(forHIDUsage: usage),
                    modifiers: [],
                    consumedModifiers: [],
                    text: nil,
                    unshiftedCodepoint: 0
                ),
                action: GHOSTTY_ACTION_RELEASE
            )
        }
        pressedKeyUsages.removeAll()
        hardwareKeyHandled = false
    }

    private func handleKeyDown(_ key: UIKey) -> Bool {
        guard let surface, let rawSurface = surface.rawValue else { return false }
        guard !inputHandler.hasMarkedText else { return false }

        let usage = UInt16(truncatingIfNeeded: key.keyCode.rawValue)
        let characters = keyText(key.characters)
        let unmodifiedCharacters = keyText(key.charactersIgnoringModifiers)
        var flags = key.modifierFlags
        if characters == nil || unmodifiedCharacters == nil {
            flags.remove(.numericPad)
        }
        let modifiers = TerminalInputModifiers(from: flags)
        let translationModifiers = TerminalInputModifiers(
            rawValue: ghostty_surface_key_translation_mods(
                rawSurface,
                modifiers.ghosttyMods
            ).rawValue
        )
        let optionAsAlt = modifiers.contains(.alt) && !translationModifiers.contains(.alt)
        let producesPlainText = characters != nil
            && modifiers.isDisjoint(with: [.ctrl, .super_])
            && !optionAsAlt
            && !TerminalHardwareKeyMap.isModifier(hidUsage: usage)
            && !Self.textSystemKeys.contains(usage)
        guard !producesPlainText else { return false }

        var text: String?
        if !modifiers.contains(.super_) {
            if optionAsAlt {
                text = unmodifiedCharacters.map {
                    translationModifiers.contains(.shift) ? $0.uppercased() : $0
                }
            } else if let characters, !Self.isControlCharacter(characters) {
                text = characters
            }
        }
        let stroke = TerminalKeyStroke(
            keyCode: TerminalHardwareKeyMap.keyCode(forHIDUsage: usage),
            modifiers: modifiers,
            consumedModifiers: translationModifiers.subtracting([.ctrl, .super_]),
            text: text,
            unshiftedCodepoint: unmodifiedCharacters?.unicodeScalars.first?.value ?? 0
        )
        TerminalDebugLog.log(
            .input,
            "uikit key down usage=0x\(String(usage, radix: 16)) mods=0x\(String(modifiers.rawValue, radix: 16)) text=\(TerminalDebugLog.describe(text ?? ""))"
        )

        if optionAsAlt
            || (modifiers.isDisjoint(with: [.ctrl, .super_]) && Self.textSystemKeys.contains(usage))
        {
            hardwareKeyHandled = true
        }
        pressedKeyUsages.insert(usage)
        send(stroke, action: GHOSTTY_ACTION_PRESS)
        if !TerminalHardwareKeyMap.isModifier(hidUsage: usage),
           !Self.textSystemKeys.contains(usage),
           !modifiers.contains(.super_)
        {
            startKeyRepeat(usage: usage, stroke: stroke)
        }
        return true
    }

    private func releaseKeys(_ presses: Set<UIPress>) -> Set<UIPress> {
        var unhandled: Set<UIPress> = []
        for press in presses {
            guard let key = press.key else {
                unhandled.insert(press)
                continue
            }
            let usage = UInt16(truncatingIfNeeded: key.keyCode.rawValue)
            guard pressedKeyUsages.remove(usage) != nil else {
                unhandled.insert(press)
                continue
            }
            if keyRepeatUsage == usage {
                stopKeyRepeat()
            }
            send(
                TerminalKeyStroke(
                    keyCode: TerminalHardwareKeyMap.keyCode(forHIDUsage: usage),
                    modifiers: TerminalInputModifiers(from: key.modifierFlags),
                    consumedModifiers: [],
                    text: nil,
                    unshiftedCodepoint: keyText(key.charactersIgnoringModifiers)?
                        .unicodeScalars.first?.value ?? 0
                ),
                action: GHOSTTY_ACTION_RELEASE
            )
        }
        return unhandled
    }

    private func startKeyRepeat(usage: UInt16, stroke: TerminalKeyStroke) {
        stopKeyRepeat()
        keyRepeatUsage = usage
        keyRepeatStroke = stroke
        let timer = Timer(
            timeInterval: Self.keyRepeatDelay,
            target: self,
            selector: #selector(keyRepeatDelayDidElapse(_:)),
            userInfo: nil,
            repeats: false
        )
        RunLoop.main.add(timer, forMode: .common)
        keyRepeatTimer = timer
    }

    @objc private func keyRepeatDelayDidElapse(_: Timer) {
        let timer = Timer(
            timeInterval: Self.keyRepeatInterval,
            target: self,
            selector: #selector(keyRepeatDidFire(_:)),
            userInfo: nil,
            repeats: true
        )
        RunLoop.main.add(timer, forMode: .common)
        keyRepeatTimer = timer
        keyRepeatDidFire(timer)
    }

    @objc private func keyRepeatDidFire(_: Timer) {
        guard let keyRepeatStroke, isFirstResponder else {
            stopKeyRepeat()
            return
        }
        send(keyRepeatStroke, action: GHOSTTY_ACTION_REPEAT)
    }

    func stopKeyRepeat() {
        keyRepeatTimer?.invalidate()
        keyRepeatTimer = nil
        keyRepeatUsage = nil
        keyRepeatStroke = nil
    }

    private func keyText(_ text: String) -> String? {
        guard !text.isEmpty, !text.hasPrefix("UIKeyInput") else { return nil }
        return TerminalInputText.filteredFunctionKeyText(text)
    }

    private static func isControlCharacter(_ text: String) -> Bool {
        guard text.unicodeScalars.count == 1,
              let scalar = text.unicodeScalars.first
        else { return false }
        return scalar.value < 0x20 || scalar.value == 0x7F
    }
}
#endif
