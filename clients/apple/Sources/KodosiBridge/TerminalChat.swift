import Foundation
import Observation
import os

@MainActor
@Observable
public final class TerminalChat {
    public private(set) var chat: ChatSnapshot?
    /// True once the pipe is open and the initial snapshot has been received.
    public private(set) var linked = false
    public private(set) var failure: String?
    /// Locally submitted answers by ask ID, until their ask is gone or a fresh snapshot arrives.
    public private(set) var answers: [String: ChatAnswer] = [:]
    public var draft = ""
    /// Counts the words that came into the draft from outside its field, which then takes the focus.
    public var drafted = 0
    /// The pictures that go with the next message.
    public private(set) var pictures: [ChatDraftPicture] = []
    /// Why the last picture that the person added did not join them.
    public private(set) var refusal: ChatDraftPicture.Refusal?

    @ObservationIgnored private let store: CanvasStore
    @ObservationIgnored let session: String
    @ObservationIgnored private var link: DocumentLink?
    @ObservationIgnored private var watchers = 0
    /// Ignores incremental updates until the host sends a full snapshot after each pipe open.
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

    public var settings: [ChatSetting] {
        chat?.settings ?? []
    }

    /// The agents of other terminals that this terminal's agent can reach, in the order to show.
    public var peers: [ChatPeer] {
        chat?.peers ?? []
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
        // Safe because `CanvasStore` delivers chat pipe events on the main actor.
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

    /// Submitting text while a prompt is pending denies the prompt with that text as feedback so
    /// the agent adjusts and continues; the pictures wait for the next message.
    public func submit() {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard canWrite else { return }
        if let question {
            guard !text.isEmpty else { return }
            draft = ""
            answer(question, .deny(text))
        } else if !text.isEmpty || !pictures.isEmpty {
            draft = ""
            request(.send(text, pictures: pictures.map(\.data)))
            pictures = []
            refusal = nil
        }
    }

    /// Adds a picture to the next message: PNG, JPEG, GIF or WebP bytes, which together stay within
    /// what one message takes. `nil` bytes are a picture that could not be read.
    public func attach(_ data: Data?) {
        guard let data, ChatDraftPicture.isPicture(data) else {
            refusal = .unreadable
            return
        }
        guard pictures.count < ChatDraftPicture.most,
              pictures.reduce(data.count, { $0 + $1.data.count }) <= ChatDraftPicture.mostBytes
        else {
            refusal = .full
            return
        }
        pictures.append(ChatDraftPicture(data: data))
        refusal = nil
    }

    public func remove(_ picture: ChatDraftPicture) {
        pictures.removeAll { $0.id == picture.id }
        refusal = nil
    }

    public func answer(_ ask: ChatAsk, _ answer: ChatAnswer) {
        guard answers[ask.id] == nil, linked else { return }
        answers[ask.id] = answer
        request(.answer(ask: ask.id, answer))
    }

    public func unqueue(_ queued: ChatQueued) {
        request(.unqueue(queued.id))
    }

    /// Asks the agent to go on with the work of `peer`'s agent, which gets no message.
    public func handOff(_ peer: ChatPeer) {
        guard canWrite else { return }
        request(.handOff(terminal: peer.terminal))
    }

    public func stop() {
        request(.stop)
    }

    /// Asks the agent for its own settings, which arrive in `settings`.
    public func askSettings() {
        request(.settings)
    }

    /// Changes one of the agent's settings to `value`, in the words of `ChatSetting.value`. The
    /// change shows at once; the host's next update keeps it or puts the setting back.
    public func set(_ setting: ChatSetting, to value: String) {
        if var chat, let index = chat.settings.firstIndex(where: { $0.key == setting.key }) {
            chat.settings[index].value = value
            chat.settings[index].refusal = nil
            self.chat = chat
        }
        request(.set(key: setting.key, value: value))
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
            // Clear pending answers: accepted answers already removed their ask on the host, while
            // any ask still present in the snapshot was never answered (e.g. lost during a drop).
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

    /// Reopens the pipe to force a fresh snapshot after a decode error or revision gap.
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
        case let .settings(settings):
            self.settings = settings
        case let .peers(peers):
            self.peers = peers
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
