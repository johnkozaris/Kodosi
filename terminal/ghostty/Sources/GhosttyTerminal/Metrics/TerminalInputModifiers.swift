import GhosttyKit
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

public struct TerminalInputModifiers: OptionSet, Sendable {
    public let rawValue: UInt32

    public init(rawValue: UInt32) {
        self.rawValue = rawValue
    }

    public static let shift = TerminalInputModifiers(rawValue: 1 << 0)
    public static let ctrl = TerminalInputModifiers(rawValue: 1 << 1)
    public static let alt = TerminalInputModifiers(rawValue: 1 << 2)
    public static let super_ = TerminalInputModifiers(rawValue: 1 << 3)
    public static let caps = TerminalInputModifiers(rawValue: 1 << 4)
    public static let num = TerminalInputModifiers(rawValue: 1 << 5)

    public var ghosttyMods: ghostty_input_mods_e {
        ghostty_input_mods_e(rawValue)
    }

    #if os(macOS)
    public init(from flags: NSEvent.ModifierFlags) {
        var mods = TerminalInputModifiers()
        if flags.contains(.shift) { mods.insert(.shift) }
        if flags.contains(.control) { mods.insert(.ctrl) }
        if flags.contains(.option) { mods.insert(.alt) }
        if flags.contains(.command) { mods.insert(.super_) }
        if flags.contains(.capsLock) { mods.insert(.caps) }
        if flags.contains(.numericPad) { mods.insert(.num) }
        self = mods
    }
    #elseif os(iOS)
    public init(from flags: UIKeyModifierFlags) {
        var mods = TerminalInputModifiers()
        if flags.contains(.shift) { mods.insert(.shift) }
        if flags.contains(.control) { mods.insert(.ctrl) }
        if flags.contains(.alternate) { mods.insert(.alt) }
        if flags.contains(.command) { mods.insert(.super_) }
        if flags.contains(.alphaShift) { mods.insert(.caps) }
        if flags.contains(.numericPad) { mods.insert(.num) }
        self = mods
    }
    #endif
}
