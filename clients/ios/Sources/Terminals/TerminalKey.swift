import SwiftUI
import UIKit

enum TerminalKey: String, Identifiable {
    case escape
    case tab
    case backtab
    case interrupt
    case left
    case up
    case down
    case right
    case one
    case two
    case three

    /// The keys that a phone's keyboard lacks, in the groups of the key row.
    static let groups: [[TerminalKey]] = [[.escape, .tab, .backtab, .interrupt], [.left, .up, .down, .right]]
    static let answers: [TerminalKey] = [.one, .two, .three]

    var id: String {
        rawValue
    }

    var usage: UIKeyboardHIDUsage {
        switch self {
        case .escape: .keyboardEscape
        case .tab, .backtab: .keyboardTab
        case .left: .keyboardLeftArrow
        case .up: .keyboardUpArrow
        case .down: .keyboardDownArrow
        case .right: .keyboardRightArrow
        case .interrupt: .keyboardC
        case .one: .keyboard1
        case .two: .keyboard2
        case .three: .keyboard3
        }
    }

    var modifiers: UIKeyModifierFlags {
        switch self {
        case .backtab: .shift
        case .interrupt: .control
        default: []
        }
    }

    var cap: String? {
        switch self {
        case .escape: "esc"
        case .tab: "tab"
        case .backtab: "⇧tab"
        case .interrupt: "⌃C"
        case .one: "1"
        case .two: "2"
        case .three: "3"
        case .left, .up, .down, .right: nil
        }
    }

    var symbol: String? {
        switch self {
        case .left: "arrow.left"
        case .up: "arrow.up"
        case .down: "arrow.down"
        case .right: "arrow.right"
        default: nil
        }
    }

    var title: LocalizedStringKey {
        switch self {
        case .escape: "Escape"
        case .tab: "Tab"
        case .backtab: "Shift Tab"
        case .left: "Left arrow"
        case .up: "Up arrow"
        case .down: "Down arrow"
        case .right: "Right arrow"
        case .interrupt: "Control C"
        case .one: "Answer 1"
        case .two: "Answer 2"
        case .three: "Answer 3"
        }
    }
}
