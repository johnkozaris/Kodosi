import AppKit
import SwiftUI

/// A key with its modifiers. `key` is the character of the key without modifiers, or its symbol.
struct KeyChord: Codable, Hashable {
    struct Modifiers: OptionSet, Codable, Hashable {
        let rawValue: Int

        static let control = Modifiers(rawValue: 1)
        static let option = Modifiers(rawValue: 2)
        static let shift = Modifiers(rawValue: 4)
        static let command = Modifiers(rawValue: 8)
    }

    private static let namedKeys: [UInt16: String] = [
        36: "↩", 76: "↩", 48: "⇥", 49: "␣", 51: "⌫", 53: "⎋", 115: "↖", 116: "⇞", 117: "⌦", 119: "↘", 121: "⇟",
        123: "←", 124: "→", 125: "↓", 126: "↑", 122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6",
        98: "F7", 100: "F8", 101: "F9", 109: "F10", 103: "F11", 111: "F12",
        18: "1", 19: "2", 20: "3", 21: "4", 23: "5", 22: "6", 26: "7", 28: "8", 25: "9", 29: "0",
    ]

    var key: String
    var modifiers: Modifiers

    init(_ key: String, _ modifiers: Modifiers = .command) {
        self.key = key
        self.modifiers = modifiers
    }

    init?(event: NSEvent) {
        let flags = event.modifierFlags
        modifiers = []
        let pairs: [(NSEvent.ModifierFlags, Modifiers)] = [(.control, .control), (.option, .option), (.shift, .shift), (.command, .command)]
        for (flag, modifier) in pairs where flags.contains(flag) {
            modifiers.insert(modifier)
        }
        if let named = Self.namedKeys[event.keyCode] {
            key = named
        } else if let text = event.characters(byApplyingModifiers: []), !text.isEmpty {
            key = text.lowercased()
        } else {
            return nil
        }
    }

    /// Without ⌘, ⌃ or ⌥ a key is text for the terminal, unless it is a function key.
    var canBeShortcut: Bool {
        !modifiers.isDisjoint(with: [.command, .control, .option]) || key.count > 1 && key.hasPrefix("F")
    }

    var label: String {
        let marks = [(Modifiers.control, "⌃"), (.option, "⌥"), (.shift, "⇧"), (.command, "⌘")]
        return marks.filter { modifiers.contains($0.0) }.map(\.1).joined() + key.uppercased()
    }

    var keyboardShortcut: KeyboardShortcut? {
        let equivalent: KeyEquivalent? = switch key {
        case "↩": .return
        case "⇥": .tab
        case "␣": .space
        case "⌫": .delete
        case "⎋": .escape
        case "↖": .home
        case "↘": .end
        case "⇞": .pageUp
        case "⇟": .pageDown
        case "⌦": .deleteForward
        case "←": .leftArrow
        case "→": .rightArrow
        case "↑": .upArrow
        case "↓": .downArrow
        default: key.count == 1 ? key.first.map { KeyEquivalent($0) } : nil
        }
        var eventModifiers: EventModifiers = []
        let pairs: [(Modifiers, EventModifiers)] = [(.control, .control), (.option, .option), (.shift, .shift), (.command, .command)]
        for (modifier, mark) in pairs where modifiers.contains(modifier) {
            eventModifiers.insert(mark)
        }
        return equivalent.map { KeyboardShortcut($0, modifiers: eventModifiers) }
    }
}

/// What a key can do in the app. The defaults are Ghostty's keys and Kodosi's own.
enum AppShortcut: String, CaseIterable, Identifiable {
    case newTerminal, quickTerminal, resume, close, minimize, zoom, details
    case previousTerminal, nextTerminal, leftTerminal, rightTerminal, upTerminal, downTerminal, numberedTerminal
    case goTo, sidebar, dosi, stopDosi, settings, fullScreen

    var id: String {
        rawValue
    }

    var title: String {
        switch self {
        case .newTerminal: String(localized: "New Terminal")
        case .quickTerminal: String(localized: "Quick Terminal")
        case .resume: String(localized: "Resume a Conversation")
        case .close: String(localized: "Close Terminal")
        case .minimize: String(localized: "Minimize")
        case .zoom: String(localized: "Zoom")
        case .details: String(localized: "Details")
        case .previousTerminal: String(localized: "Previous Terminal")
        case .nextTerminal: String(localized: "Next Terminal")
        case .leftTerminal: String(localized: "Terminal on the Left")
        case .rightTerminal: String(localized: "Terminal on the Right")
        case .upTerminal: String(localized: "Terminal Above")
        case .downTerminal: String(localized: "Terminal Below")
        case .numberedTerminal: String(localized: "Terminal 1 to 9")
        case .goTo: String(localized: "Go to")
        case .sidebar: String(localized: "Show or Hide Sidebar")
        case .dosi: String(localized: "Dosi")
        case .stopDosi: String(localized: "Stop Dosi")
        case .settings: String(localized: "Settings")
        case .fullScreen: String(localized: "Full Screen")
        }
    }

    var defaults: [KeyChord] {
        switch self {
        case .newTerminal: [KeyChord("n")]
        case .quickTerminal: [KeyChord("t"), KeyChord("d"), KeyChord("d", [.shift, .command])]
        case .resume: [KeyChord("n", [.shift, .command])]
        case .close: [KeyChord("w")]
        case .minimize: [KeyChord("w", [.shift, .command])]
        case .zoom: [KeyChord("f", [.shift, .command]), KeyChord("↩", [.shift, .command])]
        case .details: [KeyChord("i")]
        case .previousTerminal: [KeyChord("[", [.shift, .command]), KeyChord("["), KeyChord("⇥", [.control, .shift])]
        case .nextTerminal: [KeyChord("]", [.shift, .command]), KeyChord("]"), KeyChord("⇥", .control)]
        case .leftTerminal: [KeyChord("←", [.option, .command])]
        case .rightTerminal: [KeyChord("→", [.option, .command])]
        case .upTerminal: [KeyChord("↑", [.option, .command])]
        case .downTerminal: [KeyChord("↓", [.option, .command])]
        case .numberedTerminal: [KeyChord("1")]
        case .goTo: [KeyChord("k")]
        case .sidebar: [KeyChord("b")]
        case .dosi: [KeyChord("j")]
        case .stopDosi: [KeyChord(".")]
        case .settings: [KeyChord(",")]
        case .fullScreen: [KeyChord("f", [.control, .command]), KeyChord("↩")]
        }
    }

    func label(of chord: KeyChord) -> String {
        self == .numberedTerminal ? chord.label + "–9" : chord.label
    }
}

/// The keys of each action. Only what the person changed is kept.
struct ShortcutTable: Codable, Equatable {
    private var changed: [AppShortcut.RawValue: [KeyChord]] = [:]

    var isChanged: Bool {
        !changed.isEmpty
    }

    func keys(for action: AppShortcut) -> [KeyChord] {
        changed[action.rawValue] ?? action.defaults
    }

    /// The action of a key, and the digit of a numbered terminal.
    func match(_ chord: KeyChord) -> (action: AppShortcut, number: Int?)? {
        if let action = AppShortcut.allCases.first(where: { $0 != .numberedTerminal && keys(for: $0).contains(chord) }) {
            return (action, nil)
        }
        guard let number = Int(chord.key), (1 ... 9).contains(number),
              keys(for: .numberedTerminal).contains(where: { $0.modifiers == chord.modifiers })
        else { return nil }
        return (.numberedTerminal, number)
    }

    /// Gives `chord` to `action`, in place of `old`. Returns the action that had the key.
    @discardableResult
    mutating func bind(_ chord: KeyChord, to action: AppShortcut, replacing old: KeyChord? = nil) -> AppShortcut? {
        var chord = chord
        if action == .numberedTerminal {
            chord.key = "1"
        }
        let holder = AppShortcut.allCases.first { $0 != action && keys(for: $0).contains(chord) }
        if let holder {
            set(keys(for: holder).filter { $0 != chord }, for: holder)
        }
        var chords = keys(for: action)
        if let old, let index = chords.firstIndex(of: old) {
            chords[index] = chord
        } else if !chords.contains(chord) {
            chords.append(chord)
        }
        var seen = Set<KeyChord>()
        set(chords.filter { seen.insert($0).inserted }, for: action)
        return holder
    }

    mutating func remove(_ chord: KeyChord, from action: AppShortcut) {
        set(keys(for: action).filter { $0 != chord }, for: action)
    }

    private mutating func set(_ chords: [KeyChord], for action: AppShortcut) {
        changed[action.rawValue] = chords == action.defaults ? nil : chords
    }
}
