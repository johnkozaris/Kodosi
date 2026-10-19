import Foundation

public struct DosiSnapshot: Decodable, Equatable, Sendable {
    public enum State: String, Decodable, Sendable {
        case idle, working, stopping, connecting, unavailable
    }

    public var state: State
    public var setup: DosiSetup
    public var agents: [DosiAgent]
    public var instructionsEdited: Bool
    public var folder: String
    public var shared: Bool
    public var remote: DosiRemote?
    public var conversation: DosiConversation?
    public var items: [DosiItem]
    public var earlier: Bool
    public var queued: [DosiQueued]
}

public struct DosiSetup: Codable, Equatable, Sendable {
    public var agent: String?
    public var account: String?
    public var model: String?
    public var effort: String?

    public init(agent: String? = nil, account: String? = nil, model: String? = nil, effort: String? = nil) {
        self.agent = agent
        self.account = account
        self.model = model
        self.effort = effort
    }

    var json: JSONValue {
        let value: (String?) -> JSONValue = { $0.map(JSONValue.string) ?? .null }
        return .object(["agent": value(agent), "account": value(account), "model": value(model), "effort": value(effort)])
    }
}

public struct DosiAgent: Decodable, Equatable, Sendable, Identifiable {
    public let agent: String
    public let installed: Bool

    public var id: String {
        agent
    }
}

public struct DosiRemote: Decodable, Equatable, Sendable {
    public let computer: String
    public let since: Int64
}

public struct DosiConversation: Decodable, Equatable, Sendable {
    public let id: String
    public let startedAt: Int64

    public var started: Date {
        Date(timeIntervalSince1970: TimeInterval(startedAt) / 1000)
    }
}

public struct DosiQueued: Decodable, Equatable, Sendable, Identifiable {
    public let id: String
    public let text: String
}

public struct DosiItem: Decodable, Equatable, Sendable, Identifiable {
    public let id: String
    public let turn: String
    public let at: Int64
    public let body: DosiBody

    public init(id: String, turn: String, at: Int64, body: DosiBody) {
        self.id = id
        self.turn = turn
        self.at = at
        self.body = body
    }

    public var date: Date {
        Date(timeIntervalSince1970: TimeInterval(at) / 1000)
    }
}

public enum DosiBody: Decodable, Equatable, Sendable {
    case person(String)
    case text(String, live: Bool)
    case reasoning(String, live: Bool, seconds: Int?)
    case tool(DosiTool)
    case question(DosiQuestion)
    case note(String)
    case notice(DosiNotice)

    private enum Key: String, CodingKey {
        case kind, text, live, seconds
    }

    public init(from decoder: Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "person": self = try .person(fields.decode(String.self, forKey: .text))
        case "text": self = try .text(fields.decode(String.self, forKey: .text), live: fields.decode(Bool.self, forKey: .live))
        case "reasoning":
            self = try .reasoning(fields.decode(String.self, forKey: .text), live: fields.decode(Bool.self, forKey: .live),
                                  seconds: fields.decodeIfPresent(Int.self, forKey: .seconds))
        case "tool": self = try .tool(DosiTool(from: decoder))
        case "question": self = try .question(DosiQuestion(from: decoder))
        case "note": self = try .note(fields.decode(String.self, forKey: .text))
        case "notice": self = try .notice(DosiNotice(from: decoder))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown Dosi item \(kind)")
        }
    }

    public var live: Bool {
        switch self {
        case let .text(_, live), let .reasoning(_, live, _): live
        case let .tool(tool): tool.status == .running
        default: false
        }
    }
}

public struct DosiTool: Decodable, Equatable, Sendable {
    public enum Action: String, Decodable, Sendable, CaseIterable {
        case look, read, type, press, start, interrupt, close, show, wait, post, run, write, edit, search, fetch, plan, delegate, other
    }

    public enum Status: String, Decodable, Sendable {
        case running, done, failed
    }

    public let action: Action
    public let target: DosiTarget?
    public let text: String?
    public let status: Status
    public let output: String?

    public init(action: Action, target: DosiTarget?, text: String? = nil, status: Status, output: String? = nil) {
        self.action = action
        self.target = target
        self.text = text
        self.status = status
        self.output = output
    }
}

public struct DosiTarget: Decodable, Equatable, Sendable {
    public enum Kind: String, Decodable, Sendable {
        case terminal, terminals, room, rooms, agent, file, url, query, command, tool
    }

    public let kind: Kind
    public let value: String

    public init(kind: Kind, value: String) {
        self.kind = kind
        self.value = value
    }
}

public struct DosiQuestion: Decodable, Equatable, Sendable {
    public let question: String
    public let options: [String]
    public let multiple: Bool
    public let terminal: String?
    public let answer: [String]?

    public init(question: String, options: [String], multiple: Bool = false, terminal: String? = nil, answer: [String]? = nil) {
        self.question = question
        self.options = options
        self.multiple = multiple
        self.terminal = terminal
        self.answer = answer
    }

    public var open: Bool {
        answer == nil
    }
}

public struct DosiNotice: Decodable, Equatable, Sendable {
    public enum Tone: String, Decodable, Sendable {
        case stopped, failed, signIn
    }

    public let tone: Tone
    public let text: String?
}

public struct DosiInstructions: Decodable, Equatable, Sendable {
    public let text: String
    public let edited: Bool
}

public struct DosiPage: Decodable, Sendable {
    public let items: [DosiItem]
    public let earlier: Bool
}

public struct DosiContext: Equatable, Sendable {
    public var terminal: String?
    public var room: String?

    public init(terminal: String? = nil, room: String? = nil) {
        self.terminal = terminal
        self.room = room
    }

    var json: JSONValue? {
        guard terminal != nil || room != nil else { return nil }
        let value: (String?) -> JSONValue = { $0.map(JSONValue.string) ?? .null }
        return .object(["terminal": value(terminal), "room": value(room)])
    }
}
