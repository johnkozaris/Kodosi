import AppKit
@testable import KodosiDesktop
import Testing

@Test func `each default key does one thing`() {
    let chords = AppShortcut.allCases.flatMap(\.defaults)
    #expect(Set(chords).count == chords.count)
}

@Test func `a key of the table finds its action and a digit its terminal`() {
    let table = ShortcutTable()
    #expect(table.match(KeyChord("t"))?.action == .quickTerminal)
    #expect(table.match(KeyChord("d", [.shift, .command]))?.action == .quickTerminal)
    #expect(table.match(KeyChord("7"))?.action == .numberedTerminal)
    #expect(table.match(KeyChord("7"))?.number == 7)
    #expect(table.match(KeyChord("0")) == nil)
    #expect(table.match(KeyChord("c")) == nil)
}

@Test func `a key moves to the action that takes it and a change can go back`() {
    var table = ShortcutTable()
    let holder = table.bind(KeyChord("t"), to: .goTo, replacing: KeyChord("k"))
    #expect(holder == .quickTerminal)
    #expect(table.match(KeyChord("t"))?.action == .goTo)
    #expect(table.match(KeyChord("k")) == nil)
    #expect(table.keys(for: .quickTerminal) == [KeyChord("d"), KeyChord("d", [.shift, .command])])

    table.remove(KeyChord("t"), from: .goTo)
    #expect(table.keys(for: .goTo).isEmpty)
    #expect(table.isChanged)
}

@Test func `the table comes back from what it saves`() throws {
    var table = ShortcutTable()
    table.bind(KeyChord("9", [.control, .option]), to: .numberedTerminal, replacing: KeyChord("1"))
    let saved = try JSONDecoder().decode(ShortcutTable.self, from: JSONEncoder().encode(table))
    #expect(saved == table)
    #expect(saved.match(KeyChord("4", [.control, .option]))?.number == 4)
    #expect(saved.match(KeyChord("4")) == nil)
}

@Test func `a key event becomes its key without the modifiers`() throws {
    func chord(_ characters: String, code: UInt16, _ flags: NSEvent.ModifierFlags) throws -> KeyChord? {
        let event = try #require(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0, context: nil,
            characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code,
        ))
        return KeyChord(event: event)
    }
    #expect(try chord("\r", code: 36, [.command, .shift]) == KeyChord("↩", [.shift, .command]))
    #expect(try chord("", code: 123, [.command, .option]) == KeyChord("←", [.option, .command]))
    #expect(try chord("\t", code: 48, .control) == KeyChord("⇥", .control))
    #expect(try chord("&", code: 18, .command) == KeyChord("1"))
}

@Test func `plain keys stay with the terminal`() {
    #expect(!KeyChord("a", []).canBeShortcut)
    #expect(!KeyChord("a", .shift).canBeShortcut)
    #expect(KeyChord("F5", []).canBeShortcut)
    #expect(KeyChord("a", .control).canBeShortcut)
    #expect(KeyChord("↩", [.shift, .command]).label == "⇧⌘↩")
    #expect(AppShortcut.numberedTerminal.label(of: KeyChord("1")) == "⌘1–9")
    #expect(KeyChord("⇥", .control).keyboardShortcut != nil)
}

@Test func `arrow keys choose the tile beside this one`() {
    let bounds = CGRect(x: 0, y: 0, width: 1200, height: 800)
    #expect(TerminalTilesLayout.neighbor(of: 0, count: 4, in: bounds, .right) == 1)
    #expect(TerminalTilesLayout.neighbor(of: 0, count: 4, in: bounds, .down) == 2)
    #expect(TerminalTilesLayout.neighbor(of: 3, count: 4, in: bounds, .left) == 2)
    #expect(TerminalTilesLayout.neighbor(of: 3, count: 4, in: bounds, .up) == 1)
    #expect(TerminalTilesLayout.neighbor(of: 0, count: 4, in: bounds, .left) == nil)
    #expect(TerminalTilesLayout.neighbor(of: 0, count: 1, in: bounds, .right) == nil)
}
