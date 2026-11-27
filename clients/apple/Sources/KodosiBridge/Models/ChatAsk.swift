import Foundation

public struct ChatQuestion: Decodable, Equatable, Sendable {
    public let question: String
    public let header: String
    public let options: [ChatOption]
    public let multiple: Bool

    private enum Key: String, CodingKey {
        case question, header, options, multiple
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        question = try fields.decode(String.self, forKey: .question)
        header = try fields.decodeIfPresent(String.self, forKey: .header) ?? ""
        options = try fields.decode([ChatOption].self, forKey: .options)
        multiple = try fields.decodeIfPresent(Bool.self, forKey: .multiple) ?? false
    }
}

public struct ChatOption: Decodable, Equatable, Sendable {
    public let label: String
    public let description: String

    private enum Key: String, CodingKey {
        case label, description
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        label = try fields.decode(String.self, forKey: .label)
        description = try fields.decodeIfPresent(String.self, forKey: .description) ?? ""
    }
}

/// A pending agent prompt, mirrored in both chat and the terminal; the first answer wins.
public struct ChatAsk: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    public let body: AskBody
}

public enum AskBody: Decodable, Equatable, Sendable {
    /// `always` is the standing permission on offer; `helper` is the requesting subagent's task.
    case permission(ChatTool, always: AskAlways?, helper: String)
    case question([ChatQuestion])
    /// `choices` lists the offered next steps with the recommended option first.
    case plan(String, choices: [PlanChoice])

    private enum Key: String, CodingKey {
        case kind, tool, always, helper, questions, text, choices
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "permission":
            self = try .permission(fields.decode(ChatTool.self, forKey: .tool), always: fields.decodeIfPresent(AskAlways.self, forKey: .always),
                                   helper: fields.decodeIfPresent(String.self, forKey: .helper) ?? "")
        case "question": self = try .question(fields.decode([ChatQuestion].self, forKey: .questions))
        case "plan": self = try .plan(fields.decode(String.self, forKey: .text),
                                      choices: fields.decode([PlanChoice].self, forKey: .choices))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown question \(kind)")
        }
    }
}

public enum PlanChoice: String, Decodable, Sendable {
    /// Execute the plan using the current permission mode.
    case go
    /// Execute the plan, prompting before each change.
    case ask
    /// Execute the plan, allowing file edits without prompting.
    case edits
    /// Execute the plan using the agent's automated safety check.
    case auto
    /// Execute the plan without prompting for permissions.
    case alone
    /// Execute parts of the plan in parallel with autonomous subagents.
    case helpers
    /// Exit plan mode without executing the plan.
    case leave
}

public enum AskAlways: Decodable, Equatable, Sendable {
    case rule(tool: String, text: String)
    /// Switches to this permission mode for the rest of the session.
    case mode(ChatMode)
    case folder(String)

    private enum Key: String, CodingKey {
        case kind, tool, text, mode, name
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "rule": self = try .rule(tool: fields.decode(String.self, forKey: .tool), text: fields.decode(String.self, forKey: .text))
        case "mode": self = try .mode(fields.decode(ChatMode.self, forKey: .mode))
        case "folder": self = try .folder(fields.decode(String.self, forKey: .name))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown offer \(kind)")
        }
    }
}

public enum ChatMode: String, Decodable, Sendable {
    /// Prompts before file changes and commands.
    case ask
    /// Allows file edits automatically; prompts before commands.
    case edits
    /// Plans without modifying files.
    case plan
    /// Uses the agent's safety check to approve or deny calls.
    case auto
    /// Allows calls matching configured rules and denies everything else without prompting.
    case rules
    case unrestricted
}

public enum ChatRequest: Equatable, Sendable {
    case send(String)
    /// Hands the agent the work of the agent of a peer's terminal, which gets no message.
    case handOff(terminal: String)
    case unqueue(String)
    case answer(ask: String, ChatAnswer)
    case stop
    case settings
    case set(key: String, value: String)

    var json: JSONValue {
        switch self {
        case let .send(text): .object(["do": .string("send"), "text": .string(text)])
        case let .handOff(terminal): .object(["do": .string("handOff"), "terminal": .string(terminal)])
        case let .unqueue(id): .object(["do": .string("unqueue"), "id": .string(id)])
        case let .answer(ask, answer): .object(["do": .string("answer"), "ask": .string(ask), "answer": answer.json])
        case .stop: .object(["do": .string("stop")])
        case .settings: .object(["do": .string("settings")])
        case let .set(key, value): .object(["do": .string("set"), "key": .string(key), "value": .string(value)])
        }
    }
}

public enum ChatAnswer: Equatable, Sendable {
    case allow
    case always
    /// With text, passes the feedback to the agent so it continues; with `nil`, stops the turn.
    case deny(String?)
    /// One entry per question in order: selected option labels or a single free-text answer.
    case answers([[String]])
    case plan(PlanChoice)

    var json: JSONValue {
        switch self {
        case .allow: .object(["kind": .string("allow")])
        case .always: .object(["kind": .string("always")])
        case let .deny(text?): .object(["kind": .string("deny"), "text": .string(text)])
        case .deny(nil): .object(["kind": .string("deny")])
        case let .answers(answers): .object(["kind": .string("answers"), "answers": .array(answers.map { .array($0.map(JSONValue.string)) })])
        case let .plan(choice): .object(["kind": .string("plan"), "choice": .string(choice.rawValue)])
        }
    }
}
