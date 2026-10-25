enum TerminalInput {
    case reading, typing, composing

    var showsKeys: Bool {
        self == .typing
    }

    var showsAsk: Bool {
        self == .reading
    }

    func showsHeader(compact: Bool) -> Bool {
        !compact || self == .reading
    }
}

struct TerminalKeyboard {
    private var hardware: Bool
    private var wanted: Bool
    private var typing = false
    private var writing = false

    init(hardware: Bool) {
        self.hardware = hardware
        wanted = hardware
    }

    func input(composing: Bool) -> TerminalInput {
        if writing || composing {
            return .composing
        }
        return typing ? .typing : .reading
    }

    func focusesTerminal(composing: Bool) -> Bool {
        wanted && !composing && !writing
    }

    mutating func type() {
        writing = false
        wanted = true
    }

    mutating func compose() {
        writing = true
        wanted = hardware
    }

    mutating func hide() {
        writing = false
        wanted = false
    }

    mutating func hardwareChanged(_ connected: Bool) {
        if connected, !hardware {
            wanted = true
        }
        hardware = connected
    }

    mutating func terminalFocused(_ focused: Bool) {
        typing = focused
        if !focused, !writing {
            wanted = false
        }
    }

    mutating func terminalGone() {
        typing = false
    }

    mutating func composerClosed() {
        writing = false
    }
}
