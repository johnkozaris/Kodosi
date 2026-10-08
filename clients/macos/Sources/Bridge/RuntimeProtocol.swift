import Foundation

struct RuntimeEvent: Decodable, Sendable {
    let type: String
    let accountUserId: String?
    let accountEpoch: UInt64
    let payload: [String: JSONValue]

    init(from decoder: Decoder) throws {
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

    func string(_ key: String) -> String? {
        payload[key]?.stringValue
    }

    func value<T: Decodable>(_ key: String, as type: T.Type = T.self) throws -> T {
        guard let value = payload[key] else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got an incomplete answer."))
        }
        return try value.decode(as: type)
    }
}

enum RuntimeError: LocalizedError, Equatable {
    case unavailable
    case rejected(Int32)
    case invalidResponse(String)
    case operation(String)
    case timedOut
    case accountChanged

    var errorDescription: String? {
        switch self {
        case .unavailable: String(localized: "Kodosi is not ready.")
        case let .rejected(code): String(localized: "That did not work (\(code)).")
        case let .invalidResponse(message), let .operation(message): message
        case .timedOut: String(localized: "Kodosi cannot tell if that worked. Check before you try again.")
        case .accountChanged: String(localized: "Your account changed before that finished.")
        }
    }
}

struct RuntimeCommand: Encodable, Sendable {
    let type: String
    var fields: [String: JSONValue] = [:]

    func encode(to encoder: Encoder) throws {
        var value = fields
        value["type"] = .string(type)
        try value.encode(to: encoder)
    }
}

struct RuntimeCommandEnvelope<Command: Encodable>: Encodable {
    let accountUserId: String?
    let accountEpoch: UInt64
    let command: Command

    private enum CodingKeys: String, CodingKey { case accountUserId, accountEpoch }

    func encode(to encoder: Encoder) throws {
        try command.encode(to: encoder)
        var envelope = encoder.container(keyedBy: CodingKeys.self)
        try envelope.encode(accountUserId, forKey: .accountUserId)
        try envelope.encode(accountEpoch, forKey: .accountEpoch)
    }
}
