import Foundation

enum Provider: String, Codable, CaseIterable, Identifiable, Sendable {
    case claude, copilot
    var id: String {
        rawValue
    }

    var name: String {
        self == .claude ? "Claude" : "Copilot"
    }
}

struct ProviderConversationIdentity: Codable, Equatable, Hashable, Sendable {
    let provider: Provider
    let nativeConversationId: String
}

struct ProviderConversation: Decodable, Equatable, Identifiable, Sendable {
    let provider: Provider
    let nativeConversationId: String
    let workingDirectory: String
    let title: String?
    let createdAt: String?
    let updatedAt: String?
    var id: String {
        "\(provider.rawValue):\(nativeConversationId)"
    }

    var sessionName: String {
        guard let title, ProductInput.validName(title) else { return provider.name }
        return title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var identity: ProviderConversationIdentity {
        ProviderConversationIdentity(provider: provider, nativeConversationId: nativeConversationId)
    }
}

struct ProviderConversationPage: Decodable, Sendable {
    let items: [ProviderConversation]
    let nextCursor: String?
    let hasMore: Bool
    let responseBytes: Int
}

struct ConversationEntry: Decodable, Equatable, Sendable {
    let role: String
    let content: String
    let toolName: String?
    let timestamp: String?
}

struct ConversationPage: Decodable, Sendable {
    let entries: [ConversationEntry]
    let nextBeforeByte: UInt64?
    let sourceFileBytes: UInt64
    let readBytes: UInt64
    let sourceRecords: Int
    let degradedReason: String?
}

struct ProviderConfiguration: Decodable, Sendable {
    struct File: Decodable, Identifiable, Sendable {
        let label: String
        let path: String
        let exists: Bool
        let editable: Bool
        var id: String {
            path
        }
    }

    let provider: Provider
    let executable: String?
    let files: [File]
    let message: String?
}
