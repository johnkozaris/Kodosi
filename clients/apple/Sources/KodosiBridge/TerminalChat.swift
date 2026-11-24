import Foundation
import Observation
import os

/// The chat of one terminal, as the terminal's computer keeps it. The pipe is open only while a
/// view watches.
@MainActor
@Observable
public final class TerminalChat {
    public private(set) var chat: ChatSnapshot?
    /// The pipe is open and the chat as it is has come.
    public private(set) var linked = false
    public private(set) var failure: String?
    /// The answers that a person gave on this device, until their questions are gone or the chat
    /// comes again.
    public private(set) var answers: [String: ChatAnswer] = [:]
    public var draft = ""

    @ObservationIgnored private let store: CanvasStore
    @ObservationIgnored let session: String
    @ObservationIgnored private var link: DocumentLink?
    @ObservationIgnored private var watchers = 0
    /// After each opening of the pipe the host sends the whole chat first, and the changes count
    /// from it.
    @ObservationIgnored private var awaitsSnapshot = true

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    public var state: ChatState {
        chat?.state ?? .away
    }

    public var items: [ChatItem] {
        chat?.items ?? []
    }

    public var asks: [ChatAsk] {
        chat?.asks ?? []
    }

    public var queued: [ChatQueued] {
        chat?.queued ?? []
    }

    public var tasks: [ChatTask] {
        chat?.tasks ?? []
    }

    public var commands: [ChatCommand] {
        chat?.commands ?? []
    }

    /// The control of the chat that a draft of only a command's name opens in place of sending it.
    public func control(of draft: String) -> ChatControl? {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.hasPrefix("/") else { return nil }
        let name = text.dropFirst()
        return commands.first { $0.name == name }?.opens
    }

    public var facts: ChatFacts {
        chat?.facts ?? ChatFacts()
    }

    public var now: ChatNow? {
        chat?.now
    }

    /// The first question that no person on this device answered yet.
    public var question: ChatAsk? {
        asks.first { answers[$0.id] == nil }
    }

    public var working: Bool {
        state == .working || state == .asking
    }

    public var canWrite: Bool {
        linked && state != .away
    }

    public func item(_ id: String) -> ChatItem? {
        chat?.items.last { $0.id == id }
    }

    /// Each view that shows the chat watches until it ends, and they share one pipe.
    public func watch() async {
        watchers += 1
        defer {
            watchers -= 1
            if watchers == 0 {
                link?.close()
                link = nil
                linked = false
            }
        }
        if link == nil {
            await connect()
        }
        try? await Task.sleep(nanoseconds: .max)
    }

    private func connect() async {
        let link = store.chatLink(of: session)
        self.link = link
        // The canvas store gives the pipe of a chat its events on the main actor.
        link.onFrame = { [weak self] frame in
            MainActor.assumeIsolated { self?.receive(frame) }
        }
        link.onNews = { [weak self, weak link] news in
            MainActor.assumeIsolated { self?.hear(news, on: link) }
        }
        do {
            try await link.open()
        } catch {
            if self.link === link {
                self.link = nil
            }
            if !Task.isCancelled {
                failure = error.localizedDescription
            }
        }
    }

    private func hear(_ news: DocumentLink.News, on link: DocumentLink?) {
        guard self.link === link else { return }
        switch news {
        case .open:
            awaitsSnapshot = true
        case let .lost(reason):
            linked = false
            failure = reason
        case .closed:
            self.link = nil
            linked = false
        }
    }

    /// A typed message while a question waits answers it: the agent reads the words in place of
    /// what it asked to do.
    public func submit() {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, canWrite else { return }
        draft = ""
        if let question {
            answer(question, .deny(text))
        } else {
            request(.send(text))
        }
    }

    public func answer(_ ask: ChatAsk, _ answer: ChatAnswer) {
        guard answers[ask.id] == nil, linked else { return }
        answers[ask.id] = answer
        request(.answer(ask: ask.id, answer))
    }

    public func unqueue(_ queued: ChatQueued) {
        request(.unqueue(queued.id))
    }

    public func stop() {
        request(.stop)
    }

    private func request(_ request: ChatRequest) {
        guard let link, let frame = try? JSONEncoder().encode(request.json) else { return }
        link.send(frame)
    }

    func receive(_ frame: Data) {
        let decoded: ChatFrame
        do {
            decoded = try JSONDecoder().decode(ChatFrame.self, from: frame)
        } catch {
            Logger.app.error("A chat frame could not be read: \(error.localizedDescription, privacy: .public)")
            if !awaitsSnapshot {
                renew()
            }
            return
        }
        switch decoded {
        case let .snapshot(snapshot):
            awaitsSnapshot = false
            chat = snapshot
            // The host closes a question when it takes its answer, and the pipe loses what an app
            // sends while it is broken. A question that is still there waits for its answer.
            answers = [:]
            linked = true
            failure = nil
        case let .update(update):
            guard !awaitsSnapshot, var chat, update.revision > chat.revision else { return }
            guard update.revision == chat.revision + 1 else {
                renew()
                return
            }
            for change in update.changes {
                chat.apply(change)
            }
            chat.revision = update.revision
            self.chat = chat
            if !answers.isEmpty {
                answers = answers.filter { answered in chat.asks.contains { $0.id == answered.key } }
            }
        }
    }

    /// A change that went missing leaves the chat wrong, so the pipe starts again and the host sends
    /// the chat as it is.
    private func renew() {
        guard let link else { return }
        awaitsSnapshot = true
        linked = false
        link.close()
        Task { [weak self] in
            do {
                try await link.open()
            } catch {
                self?.failure = error.localizedDescription
            }
        }
    }
}

extension ChatSnapshot {
    mutating func apply(_ change: ChatChange) {
        switch change {
        case let .item(item):
            if let index = items.lastIndex(where: { $0.id == item.id }) {
                items[index] = item
            } else {
                items.append(item)
            }
        case let .text(id, more):
            append(more, to: id)
        case let .remove(id):
            items.removeAll { $0.id == id }
        case let .asks(asks):
            self.asks = asks
        case let .queued(queued):
            self.queued = queued
        case let .tasks(tasks):
            self.tasks = tasks
        case let .commands(commands):
            self.commands = commands
        case let .facts(facts):
            self.facts = facts
        case let .state(state, now):
            self.state = state
            self.now = now
        case .reset:
            items = []
            asks = []
            tasks = []
        }
    }

    private mutating func append(_ more: String, to id: String) {
        guard let index = items.lastIndex(where: { $0.id == id }), case let .text(text, live) = items[index].body else { return }
        items[index].body = .text(text + more, live: live)
    }
}
