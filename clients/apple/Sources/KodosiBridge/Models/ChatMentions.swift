import Foundation

/// A person's message: their own words, the agents of other terminals that the words name with `@`,
/// the agent whose work the message handed over, and the pictures that came with the words.
public struct ChatSaid: Decodable, Equatable, Sendable {
    public let text: String
    public let mentions: [ChatMention]
    public let handoff: ChatMention?
    public let pictures: [ChatPicture]

    private enum Key: String, CodingKey {
        case text, mentions, handoff, pictures
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        text = try fields.decode(String.self, forKey: .text)
        mentions = try fields.decodeIfPresent([ChatMention].self, forKey: .mentions) ?? []
        handoff = try fields.decodeIfPresent(ChatMention.self, forKey: .handoff)
        pictures = try fields.decodeIfPresent([ChatPicture].self, forKey: .pictures) ?? []
    }
}

/// A picture that came with a person's message, kept on the terminal's computer.
public struct ChatPicture: Decodable, Equatable, Sendable {
    /// The name of its file there; empty for a picture that the agent's terminal took.
    public let name: String

    private enum Key: String, CodingKey {
        case name
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        name = try fields.decodeIfPresent(String.self, forKey: .name) ?? ""
    }
}

/// An agent of another terminal, as a message named it to this terminal's agent.
public struct ChatMention: Decodable, Equatable, Sendable {
    /// The name of its terminal.
    public let name: String
    /// The agent's kind, such as `claude`; empty when Kodosi does not know it.
    public let agent: String

    private enum Key: String, CodingKey {
        case name, agent
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        name = try fields.decode(String.self, forKey: .name)
        agent = try fields.decodeIfPresent(String.self, forKey: .agent) ?? ""
    }
}

/// The agent of another terminal that this terminal's agent can reach.
public struct ChatPeer: Decodable, Equatable, Identifiable, Sendable {
    public let terminal: String
    public let name: String
    /// The agent's kind, such as `claude`.
    public let agent: String
    /// Its computer when that is another one, else the name of its folder.
    public let place: String

    public var id: String {
        terminal
    }

    private enum Key: String, CodingKey {
        case terminal, name, agent, place
    }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Key.self)
        terminal = try fields.decode(String.self, forKey: .terminal)
        name = try fields.decode(String.self, forKey: .name)
        agent = try fields.decode(String.self, forKey: .agent)
        place = try fields.decodeIfPresent(String.self, forKey: .place) ?? ""
    }
}
