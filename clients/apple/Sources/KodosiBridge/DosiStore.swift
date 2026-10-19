import Foundation
import Observation

@MainActor
@Observable
public final class DosiStore {
    public private(set) var snapshot: DosiSnapshot?
    public private(set) var items: [DosiItem] = []
    public private(set) var earlier = false
    public private(set) var loadingEarlier = false
    public private(set) var seenTurn: String?
    public var draft = ""

    @ObservationIgnored private let commands: CommandSink

    public init(commands: CommandSink) {
        self.commands = commands
    }

    public var state: DosiSnapshot.State {
        snapshot?.state ?? .unavailable
    }

    public var busy: Bool {
        state == .working || state == .stopping
    }

    public var queued: [DosiQueued] {
        snapshot?.queued ?? []
    }

    public var openQuestions: [DosiItem] {
        items.filter {
            if case let .question(question) = $0.body {
                question.open
            } else {
                false
            }
        }
    }

    public var latestDeed: DosiTool? {
        guard busy, let last = items.last(where: { $0.turn == currentTurn }) else { return nil }
        if case let .tool(tool) = last.body {
            return tool
        }
        return nil
    }

    public var unseen: Bool {
        guard !busy, let last = items.last, last.turn != seenTurn else { return false }
        if case .person = last.body {
            return false
        }
        return true
    }

    private var currentTurn: String? {
        items.last?.turn
    }

    public func reset() {
        snapshot = nil
        items = []
        earlier = false
        loadingEarlier = false
        seenTurn = nil
        draft = ""
    }

    public func see() {
        seenTurn = items.last?.turn
    }

    public func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "dosi.snapshot": try apply(event.value("dosi"))
        case "dosi.item": try upsert(event.value("item"))
        default: break
        }
    }

    public func question(forTerminal session: RuntimeSession) -> (DosiItem, DosiQuestion)? {
        for item in openQuestions.reversed() {
            guard case let .question(question) = item.body, let reference = question.terminal else { continue }
            if reference == session.id || reference.caseInsensitiveCompare(session.name) == .orderedSame
                || (reference.count >= 4 && session.id.hasPrefix(reference))
            {
                return (item, question)
            }
        }
        return nil
    }

    private func apply(_ next: DosiSnapshot) {
        let replaced = next.conversation?.id != snapshot?.conversation?.id
        snapshot = next
        guard !replaced, let first = next.items.first, let start = items.firstIndex(where: { $0.id == first.id }) else {
            items = next.items
            earlier = next.earlier
            return
        }
        items = Array(items[..<start]) + next.items
    }

    private func upsert(_ item: DosiItem) {
        if let index = items.firstIndex(where: { $0.id == item.id }) {
            items[index] = item
        } else {
            items.append(item)
        }
    }

    public func send(_ text: String, context: DosiContext) async throws {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        var fields: [String: JSONValue] = ["text": .string(text)]
        fields["context"] = context.json
        _ = try await commands.request("dosi.send", fields)
    }

    public func stop() {
        commands.perform("dosi.stop")
    }

    public func unqueue(_ queued: DosiQueued) {
        commands.perform("dosi.unqueue", ["queuedId": .string(queued.id)])
    }

    public func answer(_ item: DosiItem, choices: [String]) async throws {
        _ = try await commands.request("dosi.answer", ["itemId": .string(item.id), "choices": .array(choices.map(JSONValue.string))])
    }

    public func renew() async throws {
        _ = try await commands.request("dosi.new")
    }

    public func configure(_ setup: DosiSetup) async throws {
        _ = try await commands.request("dosi.setup", ["setup": setup.json])
    }

    public func instructions() async throws -> DosiInstructions {
        try await commands.request("dosi.instructions.read").value("instructions")
    }

    public func writeInstructions(_ text: String?) async throws -> DosiInstructions {
        try await commands.request("dosi.instructions.write", ["text": text.map(JSONValue.string) ?? .null]).value("instructions")
    }

    public func loadEarlier() async throws {
        guard earlier, !loadingEarlier, let first = items.first else { return }
        loadingEarlier = true
        defer { loadingEarlier = false }
        let page: DosiPage = try await commands.request("dosi.history", ["before": .string(first.id)]).value("page")
        guard items.first?.id == first.id else { return }
        items = page.items.filter { old in !items.contains { $0.id == old.id } } + items
        earlier = page.earlier
    }
}
