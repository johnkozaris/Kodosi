import Foundation

/// A host of a newer release can send a kind that this app does not know. It reads as `other`, so
/// the rest of its frame still applies.
protocol OpenKind: RawRepresentable, Decodable where RawValue == String {
    static var other: Self { get }
}

extension OpenKind {
    static func read(_ decoder: any Decoder) throws -> Self {
        try Self(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

public enum ChatFrame: Decodable, Sendable {
    case snapshot(ChatSnapshot)
    case update(ChatUpdate)
    case other

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
            self = .other
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

    private enum Key: String, CodingKey {
        case revision, state, now, facts, items, asks, queued
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
    case facts(ChatFacts)
    case state(ChatState, now: ChatNow?)
    case reset
    case other

    private enum Key: String, CodingKey {
        case change, item, id, text, asks, queued, facts, state, now
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .change) {
        case "item": self = try .item(fields.decode(ChatItem.self, forKey: .item))
        case "text": self = try .text(id: fields.decode(String.self, forKey: .id), text: fields.decode(String.self, forKey: .text))
        case "remove": self = try .remove(id: fields.decode(String.self, forKey: .id))
        case "asks": self = try .asks(fields.decode([ChatAsk].self, forKey: .asks))
        case "queued": self = try .queued(fields.decode([ChatQueued].self, forKey: .queued))
        case "facts": self = try .facts(fields.decode(ChatFacts.self, forKey: .facts))
        case "state": self = try .state(fields.decode(ChatState.self, forKey: .state), now: fields.decodeIfPresent(ChatNow.self, forKey: .now))
        case "reset": self = .reset
        default: self = .other
        }
    }
}

public enum ChatState: String, Decodable, Sendable, OpenKind {
    /// No agent of the terminal reports to the chat.
    case away
    case idle
    case working
    /// The agent waits for a person's answer.
    case asking
    case other

    public init(from decoder: any Decoder) throws {
        self = try .read(decoder)
    }
}

/// What the agent does at this moment.
public enum ChatNow: Decodable, Equatable, Sendable {
    case thinking
    case writing
    case tool(item: String)
    case compacting
    case other

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
        default: self = .other
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
    case other

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
        default: self = .other
        }
    }
}

public enum TurnEnd: String, Decodable, Sendable, OpenKind {
    case done, stopped, failed, refused, other

    public init(from decoder: any Decoder) throws {
        self = try .read(decoder)
    }
}

public enum ChatNote: Decodable, Equatable, Sendable {
    /// The agent made its conversation shorter to free its context.
    case compacted
    /// The agent could not continue. `error` is the agent's word for the cause.
    case failed(error: String, text: String)
    case other

    private enum Key: String, CodingKey {
        case kind, error, text
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        switch try fields.decode(String.self, forKey: .kind) {
        case "compacted": self = .compacted
        case "failed":
            self = try .failed(error: fields.decode(String.self, forKey: .error), text: fields.decodeIfPresent(String.self, forKey: .text) ?? "")
        default: self = .other
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
    public let ms: UInt64?

    private enum Key: String, CodingKey {
        case name, kind, state, target, detail, ms
    }

    public init(name: String, kind: ChatToolKind, state: ChatToolState, target: String = "", detail: ChatToolDetail? = nil, ms: UInt64? = nil) {
        self.name = name
        self.kind = kind
        self.state = state
        self.target = target
        self.detail = detail
        self.ms = ms
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        name = try fields.decode(String.self, forKey: .name)
        kind = try fields.decode(ChatToolKind.self, forKey: .kind)
        state = try fields.decode(ChatToolState.self, forKey: .state)
        target = try fields.decodeIfPresent(String.self, forKey: .target) ?? ""
        detail = try fields.decodeIfPresent(ChatToolDetail.self, forKey: .detail)
        ms = try fields.decodeIfPresent(UInt64.self, forKey: .ms)
    }
}

public enum ChatToolKind: String, Decodable, Sendable, CaseIterable, OpenKind {
    case command, read, edit, write, search, web
    /// A helper: another agent that the agent starts for a part of its work.
    case agent
    case skill, question, plan, other

    public init(from decoder: any Decoder) throws {
        self = try .read(decoder)
    }
}

public enum ChatToolState: String, Decodable, Sendable, OpenKind {
    /// The agent writes the call.
    case preparing
    /// The call waits for a person's permission or answer.
    case asking
    case running, done, failed, denied, stopped, other

    public init(from decoder: any Decoder) throws {
        self = try .read(decoder)
    }

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
    case other

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
        default: self = .other
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

public enum ChangeSign: String, Decodable, Sendable, OpenKind {
    case same, added, removed, other

    public init(from decoder: any Decoder) throws {
        self = try .read(decoder)
    }
}

public struct ChatQueued: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    public let text: String
    public let by: ChatPerson?
    /// The agent did not take the message, in the agent's words. The message waits until a person
    /// sends another, removes it, or writes to the agent in the terminal.
    public let refused: String?
}

/// A person of the terminal: the account, or for a person with no account the view that sent.
public struct ChatPerson: Decodable, Equatable, Sendable {
    public let id: String
    public let name: String
}

public struct ChatFacts: Decodable, Equatable, Sendable {
    /// The agent's name, as the terminal's program has it.
    public var agent: String?
    public var version: String?
    public var model: String?
    public var effort: String?
    public var mode: ChatMode?
    public var folder: String?
    public var context: ChatContext?
    public var limits: [UsageLimit]
    /// What the agent estimates that the conversation cost, in US dollars.
    public var cost: Double?
    /// Helpers that work now.
    public var helpers: Int
    /// Commands that run on after their calls ended.
    public var background: Int
    /// The models that the agent offers.
    public var models: [String]

    private enum Key: String, CodingKey {
        case agent, version, model, effort, mode, folder, context, limits, cost, helpers, background, models
    }

    public init() {
        limits = []
        helpers = 0
        background = 0
        models = []
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        agent = try fields.decodeIfPresent(String.self, forKey: .agent)
        version = try fields.decodeIfPresent(String.self, forKey: .version)
        model = try fields.decodeIfPresent(String.self, forKey: .model)
        effort = try fields.decodeIfPresent(String.self, forKey: .effort)
        mode = try fields.decodeIfPresent(ChatMode.self, forKey: .mode)
        folder = try fields.decodeIfPresent(String.self, forKey: .folder)
        context = try fields.decodeIfPresent(ChatContext.self, forKey: .context)
        limits = (try? fields.decodeIfPresent([UsageLimit].self, forKey: .limits)) ?? []
        cost = try fields.decodeIfPresent(Double.self, forKey: .cost)
        helpers = try fields.decodeIfPresent(Int.self, forKey: .helpers) ?? 0
        background = try fields.decodeIfPresent(Int.self, forKey: .background) ?? 0
        models = try fields.decodeIfPresent([String].self, forKey: .models) ?? []
    }
}

public struct ChatContext: Decodable, Equatable, Sendable {
    public let tokens: UInt64
    public let window: UInt64

    public var used: Double {
        window == 0 ? 0 : min(1, Double(tokens) / Double(window))
    }
}
