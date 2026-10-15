import GhosttyKit
#if os(macOS)
import AppKit
#endif

public struct TerminalScrollModifiers: Sendable {
    public let rawValue: ghostty_input_scroll_mods_t

    public init(precision: Bool, momentum: Momentum = .none) {
        var value: Int32 = 0
        if precision { value |= 1 }
        value |= momentum.rawValue << 1
        rawValue = value
    }

    public enum Momentum: Int32, Sendable {
        case none = 0
        case began = 1
        case stationary = 2
        case changed = 3
    }

    #if os(macOS)
    static func momentumFrom(phase: NSEvent.Phase) -> Momentum {
        if phase.contains(.began) { return .began }
        if phase.contains(.stationary) { return .stationary }
        if phase.contains(.changed) { return .changed }
        return .none
    }
    #endif
}
