import Foundation

public enum ChatFrame: Decodable, Sendable {
    case snapshot(ChatSnapshot)
    case update(ChatUpdate)

    private enum Key: String, CodingKey {
        case snapshot, update
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        if let snapshot = try fields.decodeIfPresent(ChatSnapshot.self, forKey: .snapshot) {
            self = .snapshot(snapshot)
        } else if let update = try fields.decodeIfPresent(ChatUpdate.self, forKey: .update) {
            self = .update(update)
        } else {
            throw DecodingError.dataCorrupted(DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "Unknown chat frame"))
        }
    }
}

public struct ChatSnapshot: Decodable, Equatable, Sendable {
    public var revision: UInt64
    public var state: ChatState
    public var now: ChatNow?
    public var facts: ChatFacts
    public var items: [ChatItem]
    public var asks: [ChatAsk]
    public var queued: [ChatQueued]
    public var tasks: [ChatTask]
    public var commands: [ChatCommand]

    private enum Key: String, CodingKey {
        case revision, state, now, facts, items, asks, queued, tasks, commands
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        revision = try fields.decode(UInt64.self, forKey: .revision)
        state = try fields.decode(ChatState.self, forKey: .state)
        now = try fields.decodeIfPresent(ChatNow.self, forKey: .now)
        facts = try fields.decode(ChatFacts.self, forKey: .facts)
        items = try fields.decode([ChatItem].self, forKey: .items)
        asks = try fields.decodeIfPresent([ChatAsk].self, forKey: .asks) ?? []
        queued = try fields.decodeIfPresent([ChatQueued].self, forKey: .queued) ?? []
        tasks = try fields.decodeIfPresent([ChatTask].self, forKey: .tasks) ?? []
        commands = try fields.decodeIfPresent([ChatCommand].self, forKey: .commands) ?? []
    }
}

public struct ChatUpdate: Decodable, Equatable, Sendable {
    public let revision: UInt64
    public let changes: [ChatChange]
}

public enum ChatChange: Decodable, Equatable, Sendable {
    case item(ChatItem)
    case text(id: String, text: String)
    case remove(id: String)
    case asks([ChatAsk])
    case queued([ChatQueued])
    /// The agent's list of its work, whole.
    case tasks([ChatTask])
    /// The agent's slash commands, whole.
    case commands([ChatCommand])
    case facts(ChatFacts)
    case state(ChatState, now: ChatNow?)
    case reset

    private enum Key: String, CodingKey {
        case change, item, id, text, asks, queued, tasks, commands, facts, state, now
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .change) {
        case "item": self = try .item(fields.decode(ChatItem.self, forKey: .item))
        case "text": self = try .text(id: fields.decode(String.self, forKey: .id), text: fields.decode(String.self, forKey: .text))
        case "remove": self = try .remove(id: fields.decode(String.self, forKey: .id))
        case "asks": self = try .asks(fields.decode([ChatAsk].self, forKey: .asks))
        case "queued": self = try .queued(fields.decode([ChatQueued].self, forKey: .queued))
        case "tasks": self = try .tasks(fields.decode([ChatTask].self, forKey: .tasks))
        case "commands": self = try .commands(fields.decode([ChatCommand].self, forKey: .commands))
        case "facts": self = try .facts(fields.decode(ChatFacts.self, forKey: .facts))
        case "state": self = try .state(fields.decode(ChatState.self, forKey: .state), now: fields.decodeIfPresent(ChatNow.self, forKey: .now))
        case "reset": self = .reset
        case let change:
            throw DecodingError.dataCorruptedError(forKey: .change, in: fields, debugDescription: "Unknown chat change \(change)")
        }
    }
}

/// A step of the list that the agent keeps of its work.
public struct ChatTask: Decodable, Equatable, Sendable {
    public let text: String
    public let state: TaskState

    public init(text: String, state: TaskState) {
        self.text = text
        self.state = state
    }
}

public enum TaskState: String, Decodable, Sendable {
    case pending, working, done
}

public enum ChatState: String, Decodable, Sendable {
    /// No agent of the terminal reports to the chat.
    case away
    case idle
    case working
    /// The agent waits for a person's answer.
    case asking
}

/// What the agent does at this moment.
public enum ChatNow: Decodable, Equatable, Sendable {
    case thinking
    case writing
    case tool(item: String)
    case compacting

    private enum Key: String, CodingKey {
        case kind, item
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "thinking": self = .thinking
        case "writing": self = .writing
        case "tool": self = try .tool(item: fields.decode(String.self, forKey: .item))
        case "compacting": self = .compacting
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown chat activity \(kind)")
        }
    }
}

public struct ChatItem: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    /// In milliseconds since 1970 on the terminal's computer.
    public let at: UInt64
    public var body: ChatBody
}

public enum ChatBody: Decodable, Equatable, Sendable {
    /// A message without a sender was typed in the terminal.
    case person(text: String, by: ChatPerson?)
    case text(String, live: Bool)
    case tool(ChatTool)
    /// The agent's turn ended here.
    case turn(TurnEnd, ms: UInt64)
    case note(ChatNote)

    private enum Key: String, CodingKey {
        case kind, text, by, live, tool, end, ms, note
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "person":
            self = try .person(text: fields.decode(String.self, forKey: .text), by: fields.decodeIfPresent(ChatPerson.self, forKey: .by))
        case "text": self = try .text(fields.decode(String.self, forKey: .text), live: fields.decodeIfPresent(Bool.self, forKey: .live) ?? false)
        case "tool": self = try .tool(fields.decode(ChatTool.self, forKey: .tool))
        case "turn": self = try .turn(fields.decode(TurnEnd.self, forKey: .end), ms: fields.decodeIfPresent(UInt64.self, forKey: .ms) ?? 0)
        case "note": self = try .note(fields.decode(ChatNote.self, forKey: .note))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown chat item \(kind)")
        }
    }
}

public enum TurnEnd: String, Decodable, Sendable {
    case done, stopped, failed, refused
}

public enum ChatNote: Decodable, Equatable, Sendable {
    /// The agent made its conversation shorter to free its context.
    case compacted
    /// The agent is not signed in, and signs in with its own steps in its terminal.
    case signedOut
    /// What the agent says happened while the person was away.
    case recap(String)
    /// The agent could not continue. `error` is the agent's word for the cause.
    case failed(error: String, text: String)

    private enum Key: String, CodingKey {
        case kind, error, text
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "compacted": self = .compacted
        case "signedOut": self = .signedOut
        case "recap": self = try .recap(fields.decode(String.self, forKey: .text))
        case "failed":
            self = try .failed(error: fields.decode(String.self, forKey: .error), text: fields.decodeIfPresent(String.self, forKey: .text) ?? "")
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown chat note \(kind)")
        }
    }
}

public struct ChatTool: Decodable, Equatable, Sendable {
    /// The agent's name of the tool.
    public let name: String
    public let kind: ChatToolKind
    public let state: ChatToolState
    /// The short subject: a command, the name of a file, a pattern, an address.
    public let target: String
    public let detail: ChatToolDetail?

    private enum Key: String, CodingKey {
        case name, kind, state, target, detail
    }

    public init(name: String, kind: ChatToolKind, state: ChatToolState, target: String = "", detail: ChatToolDetail? = nil) {
        self.name = name
        self.kind = kind
        self.state = state
        self.target = target
        self.detail = detail
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        name = try fields.decode(String.self, forKey: .name)
        kind = try fields.decode(ChatToolKind.self, forKey: .kind)
        state = try fields.decode(ChatToolState.self, forKey: .state)
        target = try fields.decodeIfPresent(String.self, forKey: .target) ?? ""
        detail = try fields.decodeIfPresent(ChatToolDetail.self, forKey: .detail)
    }
}

public enum ChatToolKind: String, Decodable, Sendable, CaseIterable {
    case command, read, edit, write, search, web
    /// A helper: another agent that the agent starts for a part of its work.
    case agent
    case skill, question, plan, other
}

public enum ChatToolState: String, Decodable, Sendable {
    /// The agent writes the call.
    case preparing
    /// The call waits for a person's permission or answer.
    case asking
    case running, done, failed, denied, stopped

    public var isOpen: Bool {
        self == .preparing || self == .asking || self == .running
    }
}

public enum ChatToolDetail: Decodable, Equatable, Sendable {
    /// `background`: the command runs on after its call ended.
    case command(command: String, output: String, background: Bool)
    case change(path: String, added: Int, removed: Int, lines: [ChangeLine])
    case output(String)
    /// For each question in order, what the person chose or wrote.
    case question([ChatQuestion], answers: [String])
    case plan(String)
    /// A helper of the agent: its kind's name, its steps so far, its last step and the start of
    /// what it wrote last.
    case agent(name: String, steps: Int, doing: ChatStep?, said: String)

    private enum Key: String, CodingKey {
        case kind, command, output, background, path, added, removed, lines, text, questions, answers, name, steps, doing, said
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        let text = { (key: Key) in try fields.decodeIfPresent(String.self, forKey: key) ?? "" }
        switch try fields.decode(String.self, forKey: .kind) {
        case "command":
            self = try .command(command: fields.decode(String.self, forKey: .command), output: text(.output),
                                background: fields.decodeIfPresent(Bool.self, forKey: .background) ?? false)
        case "change":
            self = try .change(path: fields.decode(String.self, forKey: .path), added: fields.decode(Int.self, forKey: .added),
                               removed: fields.decode(Int.self, forKey: .removed), lines: fields.decode([ChangeLine].self, forKey: .lines))
        case "output": self = try .output(fields.decode(String.self, forKey: .text))
        case "question":
            self = try .question(fields.decode([ChatQuestion].self, forKey: .questions),
                                 answers: fields.decodeIfPresent([String].self, forKey: .answers) ?? [])
        case "plan": self = try .plan(fields.decode(String.self, forKey: .text))
        case "agent":
            self = try .agent(name: text(.name), steps: fields.decodeIfPresent(Int.self, forKey: .steps) ?? 0,
                              doing: fields.decodeIfPresent(ChatStep.self, forKey: .doing), said: text(.said))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: fields, debugDescription: "Unknown tool detail \(kind)")
        }
    }
}

public struct ChatStep: Decodable, Equatable, Sendable {
    public let kind: ChatToolKind
    public let target: String

    private enum Key: String, CodingKey {
        case kind, target
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        kind = try fields.decode(ChatToolKind.self, forKey: .kind)
        target = try fields.decodeIfPresent(String.self, forKey: .target) ?? ""
    }
}

public struct ChangeLine: Decodable, Equatable, Sendable {
    public let sign: ChangeSign
    public let text: String
}

public enum ChangeSign: String, Decodable, Sendable {
    case same, added, removed
}

public struct ChatQueued: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    public let text: String
    /// The agent did not take the message, in the agent's words. The message waits until a person
    /// sends another, removes it, or writes to the agent in the terminal.
    public let refused: String?
    /// The agent shows this command in its terminal, where a person finishes it.
    public let terminal: Bool

    private enum Key: String, CodingKey {
        case id, text, refused, terminal
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        id = try fields.decode(String.self, forKey: .id)
        text = try fields.decode(String.self, forKey: .text)
        refused = try fields.decodeIfPresent(String.self, forKey: .refused)
        terminal = try fields.decodeIfPresent(Bool.self, forKey: .terminal) ?? false
    }
}

/// A slash command of the agent, as its own menu lists it.
public struct ChatCommand: Decodable, Equatable, Identifiable, Sendable {
    public let name: String
    public let description: String
    /// The control of the chat that does what the command does, in place of the agent's own view
    /// in its terminal.
    public let opens: ChatControl?

    public var id: String {
        name
    }

    private enum Key: String, CodingKey {
        case name, description, opens
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        name = try fields.decode(String.self, forKey: .name)
        description = try fields.decodeIfPresent(String.self, forKey: .description) ?? ""
        opens = try fields.decodeIfPresent(ChatControl.self, forKey: .opens)
    }
}

public enum ChatControl: String, Decodable, Sendable {
    /// The agent's model, effort and fast mode.
    case model
    /// The limits of the agent's plan, its context and the cost of the conversation.
    case usage
}

/// A person of the terminal: the account, or for a person with no account the view that sent.
public struct ChatPerson: Decodable, Equatable, Sendable {
    public let id: String
    public let name: String
}

public struct ChatFacts: Decodable, Equatable, Sendable {
    /// The agent of the conversation, by the name of its program. It stays after the agent ends.
    public var agent: String?
    public var model: String?
    /// The folder that the agent works in, on the terminal's computer.
    public var folder: String?
    public var effort: String?
    public var mode: ChatMode?
    public var context: ChatContext?
    public var limits: [UsageLimit]
    /// What the agent estimates that the conversation cost, in US dollars.
    public var cost: Double?
    /// Helpers that work now.
    public var helpers: Int
    /// Commands that run on after their calls ended.
    public var background: Int

    private enum Key: String, CodingKey {
        case agent, model, folder, effort, mode, context, limits, cost, helpers, background
    }

    public init() {
        limits = []
        helpers = 0
        background = 0
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        agent = try fields.decodeIfPresent(String.self, forKey: .agent)
        model = try fields.decodeIfPresent(String.self, forKey: .model)
        folder = try fields.decodeIfPresent(String.self, forKey: .folder)
        effort = try fields.decodeIfPresent(String.self, forKey: .effort)
        mode = try fields.decodeIfPresent(ChatMode.self, forKey: .mode)
        context = try fields.decodeIfPresent(ChatContext.self, forKey: .context)
        limits = try fields.decodeIfPresent([UsageLimit].self, forKey: .limits) ?? []
        cost = try fields.decodeIfPresent(Double.self, forKey: .cost)
        helpers = try fields.decodeIfPresent(Int.self, forKey: .helpers) ?? 0
        background = try fields.decodeIfPresent(Int.self, forKey: .background) ?? 0
    }
}

public struct ChatContext: Decodable, Equatable, Sendable {
    public let tokens: UInt64
    public let window: UInt64

    public var used: Double {
        window == 0 ? 0 : min(1, Double(tokens) / Double(window))
    }
}
