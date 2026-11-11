import Foundation
import Observation

public enum CanvasNews: Equatable, Sendable {
    case shown(CanvasShown)
    case changed(CanvasShown)

    public var shown: CanvasShown {
        switch self {
        case let .shown(shown), let .changed(shown): shown
        }
    }
}

@MainActor
@Observable
public final class CanvasStore {
    private struct Stored: Hashable {
        let session: String
        let key: String
        let revision: UInt64
    }

    private struct Answer: Decodable {
        let answer: String
        let key: String?
        let revision: UInt64?
        let offset: UInt64?
        let total: UInt64?
        let data: String?
        let text: String?
    }

    private static let mostStoredBytes = 64 * 1024 * 1024
    private static let mostFileBytes = 32 * 1024 * 1024
    private static let partBytes = 256 * 1024

    private let commands: CommandSink
    private var marks: [String: [String: UInt64]] = [:]
    private var firstSeen: [String: UInt64] = [:]
    public private(set) var picks: [String: String] = [:]
    /// The terminals whose canvas shows all its files as cards.
    public private(set) var shelves: Set<String> = []
    private var lastFiles: [String: String] = [:]
    /// The part of each item that this device read so far, while it reads.
    public private(set) var reads: [String: Double] = [:]
    /// The terminals whose browser a person on this device opens: the canvas shows it when it comes.
    private var opening: Set<String> = []
    /// The parts of each canvas that a person closed on this device, with the newest change in
    /// each part then. A part opens again when it changes.
    private var closed: [String: [CanvasSection: UInt64]] = [:]
    /// The time of the newest show of each terminal's agent that this device acted on, or the start
    /// of the app: a show from before is not news.
    private var opened: [String: UInt64] = [:]
    /// The same for the changes of each canvas.
    private var noticed: [String: UInt64] = [:]
    /// The terminals whose canvas a person closed on this device since it last opened or its agent
    /// last showed something: a change does not open it.
    private var shut: Set<String> = []
    /// When the app started, in milliseconds since 1970.
    private let started: UInt64
    @ObservationIgnored private var stored: [Stored: Data] = [:]
    @ObservationIgnored private var storedOrder: [Stored] = []
    @ObservationIgnored private var reading: [Stored: Task<(UInt64, Data), any Error>] = [:]
    @ObservationIgnored private var feeds: [String: BrowserFeed] = [:]
    @ObservationIgnored private let documents = NSHashTable<DocumentLink>.weakObjects()
    /// The terminals that ran in the last list. A link of a terminal that runs again joins at once.
    @ObservationIgnored private var running: Set<String> = []

    public init(commands: CommandSink, started: UInt64 = UInt64(Date.now.timeIntervalSince1970 * 1000)) {
        self.commands = commands
        self.started = started
    }

    func browser(of session: String) -> DocumentLink {
        let link = DocumentLink(commands: commands, session: session, key: CanvasShown.browserKey)
        documents.add(link)
        return link
    }

    public func document(_ item: CanvasItem, in session: String) -> DocumentLink {
        let link = DocumentLink(commands: commands, session: session, key: item.key)
        documents.add(link)
        return link
    }

    public func receive(_ event: RuntimeEvent) {
        for link in documents.allObjects {
            link.receive(event)
        }
    }

    public func observe(_ sessions: [RuntimeSession]) {
        let ids = Set(sessions.map(\.id))
        for session in sessions where firstSeen[session.id] == nil {
            firstSeen[session.id] = session.canvas?.newest?.changed ?? 0
        }
        for session in sessions where opened[session.id] == nil {
            opened[session.id] = started
            noticed[session.id] = started
        }
        firstSeen = firstSeen.filter { ids.contains($0.key) }
        marks = marks.filter { ids.contains($0.key) }
        for session in sessions {
            if opening.contains(session.id), session.canvas?.browser != nil {
                opening.remove(session.id)
                picks[session.id] = CanvasShown.browserKey
            }
            if let pick = picks[session.id], session.canvas?.shown(pick) == nil {
                picks[session.id] = nil
            }
        }
        opening.formIntersection(ids)
        shut.formIntersection(ids)
        closed = closed.filter { ids.contains($0.key) }
        picks = picks.filter { ids.contains($0.key) }
        lastFiles = lastFiles.filter { ids.contains($0.key) }
        shelves.formIntersection(ids)
        let browsing = Set(sessions.filter { $0.canvas?.browser != nil }.map(\.id))
        feeds = feeds.filter { browsing.contains($0.key) }
        for key in stored.keys where !ids.contains(key.session) {
            forget(key)
        }
        let runs = Set(sessions.filter { $0.status == .running }.map(\.id))
        for link in documents.allObjects where runs.contains(link.session) && !running.contains(link.session) {
            link.reachable()
        }
        running = runs
    }

    public func isNew(_ shown: CanvasShown, in session: String) -> Bool {
        shown.changed > (marks[session]?[shown.key] ?? firstSeen[session] ?? 0)
    }

    public func hasNews(_ session: RuntimeSession) -> Bool {
        session.canvas?.shown.contains { isNew($0, in: session.id) } ?? false
    }

    public func saw(_ shown: CanvasShown, in session: String) {
        if picks[session] == nil {
            picks[session] = shown.key
        }
        guard isNew(shown, in: session) else { return }
        marks[session, default: [:]][shown.key] = shown.changed
    }

    public func current(_ session: RuntimeSession) -> CanvasShown? {
        guard let canvas = session.canvas else { return nil }
        let open = sections(session)
        if let pick = picks[session.id].flatMap(canvas.shown), open.contains(pick.section) {
            return pick
        }
        return canvas.shown.filter { open.contains($0.section) }.max { $0.changed < $1.changed }
    }

    public func sections(_ session: RuntimeSession) -> [CanvasSection] {
        guard let canvas = session.canvas else { return [] }
        return canvas.sections.filter { section in
            closed[session.id]?[section].map { (canvas.changed(of: section) ?? 0) > $0 } ?? true
        }
    }

    public func showsSomething(_ session: RuntimeSession) -> Bool {
        !sections(session).isEmpty
    }

    public func closedSections(_ session: RuntimeSession) -> [CanvasSection] {
        let open = sections(session)
        return [CanvasSection.files, .browser].filter { section in
            !open.contains(section) && (section == .browser || session.canvas?.sections.contains(section) == true)
        }
    }

    public func close(_ section: CanvasSection, in session: RuntimeSession) {
        guard let canvas = session.canvas else { return }
        settle(session)
        closed[session.id, default: [:]][section] = canvas.changed(of: section) ?? 0
        if picks[session.id].flatMap(canvas.shown)?.section == section {
            picks[session.id] = nil
        }
        if section == .files {
            shelves.remove(session.id)
        }
    }

    public func open(_ session: RuntimeSession, at section: CanvasSection? = nil) {
        shut.remove(session.id)
        if let section {
            show(section, in: session)
        } else if !showsSomething(session), let newest = session.canvas?.newest {
            show(newest.section, in: session)
        }
    }

    public func news(_ session: RuntimeSession) -> [CanvasNews] {
        guard let canvas = session.canvas else { return [] }
        var news: [(at: UInt64, news: CanvasNews)] = []
        if let open = canvas.open, open.at > opened[session.id] ?? started {
            opened[session.id] = open.at
            if let shown = canvas.shown(open.key) {
                shut.remove(session.id)
                news.append((open.at, .shown(shown)))
            }
        }
        if let change = canvas.change, change.at > noticed[session.id] ?? started {
            noticed[session.id] = change.at
            if let shown = canvas.shown(change.key) {
                news.append((change.at, .changed(shown)))
            }
        }
        return news.sorted { $0.at < $1.at }.map(\.news)
    }

    public func reveal(_ shown: CanvasShown, in session: RuntimeSession) {
        settle(session)
        shut.remove(session.id)
        closed[session.id]?[shown.section] = nil
        pick(shown.key, in: session.id)
    }

    public func closeCanvas(_ session: String) {
        shut.insert(session)
    }

    public func isShut(_ session: String) -> Bool {
        shut.contains(session)
    }

    private func settle(_ session: RuntimeSession) {
        if let at = session.canvas?.open?.at {
            opened[session.id] = max(opened[session.id] ?? started, at)
        }
        if let at = session.canvas?.change?.at {
            noticed[session.id] = max(noticed[session.id] ?? started, at)
        }
    }

    public func feed(_ session: String) -> BrowserFeed {
        if let feed = feeds[session] {
            return feed
        }
        let feed = BrowserFeed(store: self, session: session)
        feeds[session] = feed
        return feed
    }

    public func pick(_ key: String?, in session: String) {
        picks[session] = key
        if let key, key != CanvasShown.browserKey {
            lastFiles[session] = key
            shelves.remove(session)
        }
    }

    public func show(_ section: CanvasSection, in session: RuntimeSession) {
        settle(session)
        shut.remove(session.id)
        closed[session.id]?[section] = nil
        switch section {
        case .browser:
            pick(CanvasShown.browserKey, in: session.id)
        case .files:
            let canvas = session.canvas
            picks[session.id] = lastFiles[session.id].flatMap { canvas?.shown($0) == nil ? nil : $0 }
                ?? canvas?.items.max { $0.changed < $1.changed }?.key
        }
    }

    public func showsShelf(_ session: String) -> Bool {
        shelves.contains(session)
    }

    public func showShelf(_ shown: Bool, in session: String) {
        if shown {
            shelves.insert(session)
        } else {
            shelves.remove(session)
        }
    }

    public func stored(_ item: CanvasItem, in session: String) -> Data? {
        stored[Stored(session: session, key: item.key, revision: item.revision)]
    }

    public func read(_ key: String, in session: String) -> Double? {
        reads[Self.place(session, key)]
    }

    private static func place(_ session: String, _ key: String) -> String {
        session + "\u{1F}" + key
    }

    public func content(of item: CanvasItem, in session: String) async throws -> (revision: UInt64, data: Data) {
        let key = Stored(session: session, key: item.key, revision: item.revision)
        if let data = stored[key] {
            storedOrder.removeAll { $0 == key }
            storedOrder.append(key)
            return (item.revision, data)
        }
        if let read = reading[key] {
            return try await read.value
        }
        let read = Task { try await self.read(item, in: session) }
        reading[key] = read
        defer { reading[key] = nil }
        let (revision, data) = try await read.value
        store(Stored(session: session, key: item.key, revision: revision), data)
        return (revision, data)
    }

    public func browserStatus() async throws -> BrowserStatus {
        let event = try await commands.request("browser.status", [:])
        return try BrowserStatus(sources: event.value("sources"), sites: event.value("sites"), engine: event.string("engine"),
                                 getting: try? event.value("getting", as: Int.self))
    }

    public func getBrowser() async throws {
        _ = try await commands.request("browser.get", [:])
    }

    public func copySignIns(from source: String) async throws -> Int {
        try await commands.request("browser.copy", ["name": .string(source)]).value("sites")
    }

    public func removeCopiedSignIns() async throws -> Int {
        try await commands.request("browser.forget", [:]).value("sites")
    }

    public func removeBrowsingData() async throws {
        _ = try await commands.request("browser.clear", [:])
    }

    public func opensBrowser(_ session: String) -> Bool {
        opening.contains(session)
    }

    public func openBrowser(of session: RuntimeSession) async throws {
        guard session.canvas?.browser == nil else {
            show(.browser, in: session)
            return
        }
        shut.remove(session.id)
        closed[session.id]?[.browser] = nil
        opening.insert(session.id)
        do {
            try await act(.wake, in: session.id)
        } catch {
            opening.remove(session.id)
            throw error
        }
    }

    public func act(_ act: BrowserAct, in session: String) async throws {
        _ = try await ask(session, .object(["type": .string("act"), "action": act.json]))
    }

    public func attach(_ file: URL, in session: String) async throws {
        try await pick(upload(file, to: session), in: session)
    }

    func upload(_ file: URL, to session: String, page: Bool = false) async throws -> String? {
        let granted = file.startAccessingSecurityScopedResource()
        defer {
            if granted {
                file.stopAccessingSecurityScopedResource()
            }
        }
        let data = try Data(contentsOf: file, options: .mappedIfSafe)
        guard data.count <= Self.mostFileBytes else {
            throw RuntimeError.operation(String(localized: "The canvas takes files up to 32 MB.", bundle: .module))
        }
        if !page {
            closed[session]?[.files] = nil
        }
        let upload = UUIDv7.generate()
        var offset = 0
        var key: String?
        repeat {
            let end = min(offset + Self.partBytes, data.count)
            key = try await ask(session, .object([
                "type": .string("attach"), "upload": .string(upload), "name": .string(file.lastPathComponent),
                "offset": .uint(UInt64(offset)), "data": .string(data[offset ..< end].base64EncodedString()),
                "last": .bool(end == data.count), "page": .bool(page),
            ])).key
            offset = end
        } while offset < data.count
        return key
    }

    public func remove(_ keys: [String], in session: String) async throws {
        _ = try await ask(session, .object(["type": .string("remove"), "keys": .array(keys.map(JSONValue.string))]))
    }

    private func ask(_ session: String, _ request: JSONValue) async throws -> Answer {
        let event = try await commands.request("canvas.request", ["sessionId": .string(session), "request": request])
        return try event.value("answer", as: Answer.self)
    }

    private func read(_ item: CanvasItem, in session: String) async throws -> (UInt64, Data) {
        let place = Self.place(session, item.key)
        defer { reads[place] = nil }
        var revision = item.revision
        var data = Data()
        var total: UInt64?
        for _ in 0 ..< 4 {
            while total.map({ UInt64(data.count) < $0 }) ?? true {
                let answer = try await ask(session, .object([
                    "type": .string("read"), "key": .string(item.key), "revision": .uint(revision), "offset": .uint(UInt64(data.count)),
                ]))
                if answer.answer == "stale", let newer = answer.revision {
                    revision = newer
                    data = Data()
                    total = nil
                    break
                }
                guard answer.answer == "part", answer.offset == UInt64(data.count), let size = answer.total,
                      let encoded = answer.data, let part = Data(base64Encoded: encoded),
                      !part.isEmpty || size == UInt64(data.count)
                else { throw CanvasError.unreadable }
                data.append(part)
                total = size
                reads[place] = size > 0 ? Double(data.count) / Double(size) : 1
            }
            if let total, UInt64(data.count) == total {
                return (revision, data)
            }
        }
        throw CanvasError.unreadable
    }

    private func store(_ key: Stored, _ data: Data) {
        forget(key)
        stored[key] = data
        storedOrder.append(key)
        var bytes = stored.values.reduce(0) { $0 + $1.count }
        while bytes > Self.mostStoredBytes, storedOrder.count > 1 {
            let oldest = storedOrder.removeFirst()
            bytes -= stored.removeValue(forKey: oldest)?.count ?? 0
        }
    }

    private func forget(_ key: Stored) {
        stored[key] = nil
        storedOrder.removeAll { $0 == key }
    }
}
