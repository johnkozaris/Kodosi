import SwiftUI

enum StableHash {
    static func value(_ text: String) -> UInt64 {
        var hash: UInt64 = 0xCBF2_9CE4_8422_2325
        for byte in text.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01B3
        }
        return hash
    }

    static func pick<Element>(_ text: String, from values: [Element]) -> Element {
        values[Int(value(text) % UInt64(values.count))]
    }
}

enum PersonTint {
    private static let palette: [Color] = [
        hex(0x3C8DB5), hex(0x8573D1), hex(0x2F92A3), hex(0xC2659A),
        hex(0xB58428), hex(0xB362B6), hex(0x607FCC),
    ]

    static func color(for key: String) -> Color {
        StableHash.pick(key, from: palette)
    }
}

enum AgentKind: String {
    case claude, codex, copilot, cursor, shell

    init(program: String?) {
        self = program.flatMap { AgentKind(rawValue: $0) } ?? .shell
    }

    init(agentName: String?, program: String? = nil) {
        if let kind = program.flatMap({ AgentKind(rawValue: $0) }) {
            self = kind
            return
        }
        let name = agentName?.lowercased() ?? ""
        self = [AgentKind.claude, .codex, .copilot, .cursor].first { name.contains($0.rawValue) } ?? .shell
    }

    var asset: String? {
        switch self {
        case .claude: "ProviderClaude"
        case .codex: "ProviderCodex"
        case .copilot: "ProviderCopilot"
        case .cursor: "ProviderCursor"
        case .shell: nil
        }
    }

    var isAgent: Bool {
        self != .shell
    }

    var tint: Color {
        switch self {
        case .claude: hex(0xD97757)
        case .codex: hex(0x5F6B7A)
        case .copilot: hex(0x8660D9)
        case .cursor: hex(0x4E7C8C)
        case .shell: hex(0x54433A)
        }
    }

    var label: String {
        switch self {
        case .claude: "Claude Code"
        case .codex: "Codex"
        case .copilot: "Copilot"
        case .cursor: "Cursor"
        case .shell: String(localized: "Shell")
        }
    }
}

enum RoomTint {
    private static let palette: [Color] = [
        hex(0xD98A5F), hex(0xD9A441), hex(0xC96F7E), hex(0x8C7BD1),
        hex(0x4F9BB0), hex(0xB8743C), hex(0xA86AAE),
    ]

    static func color(for key: String) -> Color {
        StableHash.pick(key, from: palette)
    }
}
