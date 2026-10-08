import Foundation
import Observation

@MainActor
@Observable
final class ConversationBrowser {
    var provider: Provider = .claude
    var directory = ""
    private(set) var conversations: [ProviderConversation] = []
    private(set) var selected: ProviderConversation?
    private(set) var entries: [ConversationEntry] = []
    private(set) var nextCursor: String?
    private(set) var nextBeforeByte: UInt64?
    private(set) var loading = false
    private(set) var reading = false
    private(set) var sourceFileBytes: UInt64 = 0
    private(set) var pageNumber = 1
    private var pageEnds: [UInt64?] = [nil]
    var canReadNewer: Bool {
        pageNumber > 1
    }

    var message: String?
    private var generation: UInt64 = 0
    private var readGeneration: UInt64 = 0
    private var task: Task<Void, Never>?
    private var readTask: Task<Void, Never>?
    private let commands: CommandSink

    init(commands: CommandSink, directory: String) {
        self.commands = commands
        self.directory = directory
    }

    private func discoveryFields(more: Bool) -> [String: JSONValue] {
        var fields: [String: JSONValue] = [
            "provider": .string(provider.rawValue),
            "limit": .int(50), "maxBytes": .int(131_072),
        ]
        if !directory.isEmpty {
            fields["workingDirectory"] = .string(directory)
        }
        if more, let nextCursor {
            fields["cursor"] = .string(nextCursor)
        }
        return fields
    }

    func load(more: Bool = false) {
        guard !more || nextCursor != nil else { return }
        task?.cancel()
        generation &+= 1
        let generation = generation
        let fields = discoveryFields(more: more)
        if !more {
            conversations.removeAll(); selected = nil; entries.removeAll(); nextCursor = nil; cancelRead()
        }
        loading = true
        message = nil
        task = Task { @MainActor [weak self] in
            guard let self else { return }
            defer {
                if self.generation == generation {
                    loading = false
                }
            }
            do {
                let reply = try await commands.request("provider.discoverConversations", fields)
                let page: ProviderConversationPage = try reply.value("result")
                guard !Task.isCancelled, self.generation == generation else { return }
                if more {
                    let existing = Set(conversations.map(\.id))
                    conversations.append(contentsOf: page.items.filter { !existing.contains($0.id) })
                } else {
                    conversations = page.items
                }
                if conversations.count >= 1000 {
                    conversations = Array(conversations.prefix(1000))
                    nextCursor = nil
                    message = String(localized: "Showing the first 1,000 conversations. Choose a project folder to narrow the list.")
                } else {
                    nextCursor = page.nextCursor
                }
            } catch {
                guard !Task.isCancelled, self.generation == generation else { return }
                message = error.localizedDescription
            }
        }
    }

    func select(_ conversation: ProviderConversation) {
        cancelRead()
        selected = conversation
        message = nil
        pageEnds = [nil]
        pageNumber = 1
        sourceFileBytes = 0
        entries.removeAll()
        nextBeforeByte = nil
        read(older: false)
    }

    func read(older: Bool) {
        guard !reading else { return }
        var ends = pageEnds
        var number = pageNumber
        if older {
            guard let nextBeforeByte else { return }
            ends = Array(ends.prefix(number)) + [nextBeforeByte]
            number += 1
        }
        loadPage(ends: ends, number: number)
    }

    private func loadPage(ends: [UInt64?], number: Int) {
        guard let selected else { return }
        readTask?.cancel()
        readGeneration &+= 1
        let generation = readGeneration
        var fields: [String: JSONValue] = [
            "provider": .string(selected.provider.rawValue),
            "workingDirectory": .string(selected.workingDirectory),
            "nativeConversationId": .string(selected.nativeConversationId),
            "limit": .int(50), "maxBytes": .int(131_072),
        ]
        if let end = ends[number - 1] {
            fields["beforeByte"] = .uint(end)
        }
        reading = true
        readTask = Task { @MainActor [weak self] in
            guard let self else { return }
            defer {
                if readGeneration == generation {
                    reading = false
                }
            }
            do {
                let reply = try await commands.request("provider.readConversation", fields)
                let page: ConversationPage = try reply.value("result")
                guard !Task.isCancelled, readGeneration == generation, self.selected?.id == selected.id else { return }
                pageEnds = ends
                pageNumber = number
                entries = page.entries
                sourceFileBytes = page.sourceFileBytes
                nextBeforeByte = page.nextBeforeByte
                message = page.degradedReason

            } catch {
                guard !Task.isCancelled, readGeneration == generation else { return }
                message = error.localizedDescription
            }
        }
    }

    func newer() {
        guard canReadNewer, !reading else { return }
        loadPage(ends: pageEnds, number: pageNumber - 1)
    }

    func latest() {
        guard !reading else { return }
        loadPage(ends: [nil], number: 1)
    }

    func cancel() {
        task?.cancel(); task = nil; generation &+= 1; loading = false; cancelRead()
    }

    private func cancelRead() {
        readTask?.cancel(); readTask = nil; readGeneration &+= 1; reading = false
    }
}
