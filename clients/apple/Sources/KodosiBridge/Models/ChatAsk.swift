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

/// A question of the agent that is open. The agent shows the same question in its terminal, and
/// the first answer counts.
public struct ChatAsk: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    /// The item of the tool call that waits.
    public let item: String?
    public let body: AskBody
}

public enum AskBody: Decodable, Equatable, Sendable {
    /// `always` is what the agent offers to allow from now on, and `helper` the task of the
    /// helper that asks.
    case permission(ChatTool, always: AskAlways?, helper: String)
    case question([ChatQuestion])
    /// With the ways on that the agent offers, the one that it recommends first.
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

/// A way on after a plan of the agent.
public enum PlanChoice: String, Decodable, Sendable {
    /// Carry out the plan, and the agent asks as it is set.
    case go
    /// Carry out the plan, and the agent asks before each change.
    case ask
    /// Carry out the plan, and the agent changes files without asking.
    case edits
    /// The agent carries out the plan alone and asks nothing.
    case alone
    /// Helpers of the agent carry out parts of the plan side by side, alone.
    case helpers
    /// The agent leaves plan mode and does not carry out the plan.
    case leave
}

public enum AskAlways: Decodable, Equatable, Sendable {
    /// Calls that match this rule, such as a command.
    case rule(tool: String, text: String)
    /// Each call that this permission mode allows, until the session ends.
    case mode(ChatMode)
    case folder(String)

    private enum Key: String, CodingKey {
        case kind, tool, text, mode, path
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "rule": self = try .rule(tool: fields.decode(String.self, forKey: .tool), text: fields.decode(String.self, forKey: .text))
        case "mode": self = try .mode(fields.decode(ChatMode.self, forKey: .mode))
        case "folder": self = try .folder(fields.decode(String.self, forKey: .path))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown offer \(kind)")
        }
    }
}

/// When the agent asks before it acts.
public enum ChatMode: String, Decodable, Sendable {
    /// Before each change and each command.
    case ask
    /// Before each command, and not before a change of a file.
    case edits
    /// The agent plans and changes nothing.
    case plan
    /// The agent decides which calls are safe.
    case auto
    /// The agent does what a rule allows and refuses the rest. It asks nothing.
    case rules
    case unrestricted
}

/// What the app sends on the chat pipe.
public enum ChatRequest: Equatable, Sendable {
    /// A message for the agent. It waits in the queue while the agent works.
    case send(String)
    case unqueue(String)
    case answer(ask: String, ChatAnswer)
    /// Stop the agent's turn and the commands of the turn.
    case stop

    var json: JSONValue {
        switch self {
        case let .send(text): .object(["do": .string("send"), "text": .string(text)])
        case let .unqueue(id): .object(["do": .string("unqueue"), "id": .string(id)])
        case let .answer(ask, answer): .object(["do": .string("answer"), "ask": .string(ask), "answer": answer.json])
        case .stop: .object(["do": .string("stop")])
        }
    }
}

public enum ChatAnswer: Equatable, Sendable {
    case allow
    /// Allow, with what the question offers to allow from now on.
    case always
    /// With words, the agent reads them and continues. Without, the agent stops.
    case deny(String?)
    /// For each question in order, the labels that the person chose or one text of their own.
    case answers([[String]])
    /// The way on after a plan that the person chose.
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
