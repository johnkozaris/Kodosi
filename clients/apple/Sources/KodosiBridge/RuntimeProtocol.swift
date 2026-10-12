import Foundation

public struct RuntimeEvent: Decodable, Sendable {
    public let type: String
    public let accountUserId: String?
    public let accountEpoch: UInt64
    public let payload: [String: JSONValue]

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let fields = try container.decode([String: JSONValue].self)
        let envelope = try decoder.container(keyedBy: CodingKeys.self)
        guard let type = fields["type"]?.stringValue, !type.isEmpty,
              fields.keys.contains("accountUserId")
        else {
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Invalid runtime event envelope")
        }
        accountEpoch = try envelope.decode(UInt64.self, forKey: .accountEpoch)
        switch fields["accountUserId"] {
        case .null?: accountUserId = nil
        case let .string(id)?: accountUserId = id
        default:
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Invalid account identity")
        }
        self.type = type
        payload = fields
    }

    private enum CodingKeys: String, CodingKey { case accountEpoch }

    public func string(_ key: String) -> String? {
        payload[key]?.stringValue
    }

    public func value<T: Decodable>(_ key: String, as type: T.Type = T.self) throws -> T {
        guard let value = payload[key] else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got an incomplete answer.", bundle: .module))
        }
        return try value.decode(as: type)
    }
}

public enum RuntimeError: LocalizedError, Equatable {
    case unavailable
    case rejected(Int32)
    case invalidResponse(String)
    case operation(String)
    case timedOut
    case accountChanged

    public var errorDescription: String? {
        switch self {
        case .unavailable: String(localized: "Kodosi is not ready.", bundle: .module)
        case let .rejected(code): String(localized: "That did not work (\(code)).", bundle: .module)
        case let .invalidResponse(message), let .operation(message): message
        case .timedOut: String(localized: "Kodosi cannot tell if that worked. Check before you try again.", bundle: .module)
        case .accountChanged: String(localized: "Your account changed before that finished.", bundle: .module)
        }
    }
}

public struct RuntimeCommand: Encodable, Sendable {
    public let type: String
    public var fields: [String: JSONValue] = [:]

    public init(type: String, fields: [String: JSONValue] = [:]) {
        self.type = type
        self.fields = fields
    }

    public func encode(to encoder: Encoder) throws {
        var value = fields
        value["type"] = .string(type)
        try value.encode(to: encoder)
    }
}

public struct RuntimeCommandEnvelope<Command: Encodable>: Encodable {
    public let accountUserId: String?
    public let accountEpoch: UInt64
    public let command: Command

    public init(accountUserId: String?, accountEpoch: UInt64, command: Command) {
        self.accountUserId = accountUserId
        self.accountEpoch = accountEpoch
        self.command = command
    }

    private enum CodingKeys: String, CodingKey { case accountUserId, accountEpoch }

    public func encode(to encoder: Encoder) throws {
        try command.encode(to: encoder)
        var envelope = encoder.container(keyedBy: CodingKeys.self)
        try envelope.encode(accountUserId, forKey: .accountUserId)
        try envelope.encode(accountEpoch, forKey: .accountEpoch)
    }
}
