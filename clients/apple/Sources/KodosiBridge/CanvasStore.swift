import Foundation
import Observation

/// What this device knows of the canvases of the terminals: what a person saw and picked, and
/// the content of the items that it showed.
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
        let revision: UInt64?
        let offset: UInt64?
        let total: UInt64?
        let data: String?
        let sequence: UInt64?
        let frame: AnsweredFrame?
        let text: String?
    }

    private struct AnsweredFrame: Decodable {
        let width: Int
        let height: Int
        let data: String
    }

    private static let mostStoredBytes = 64 * 1024 * 1024

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
    @ObservationIgnored private var stored: [Stored: Data] = [:]
    @ObservationIgnored private var storedOrder: [Stored] = []
    @ObservationIgnored private var reading: [Stored: Task<(UInt64, Data), any Error>] = [:]
    @ObservationIgnored private var feeds: [String: BrowserFeed] = [:]

    public init(commands: CommandSink) {
        self.commands = commands
    }

    /// Follows the terminal list. What a canvas showed before this device saw it is not news.
    public func observe(_ sessions: [RuntimeSession]) {
        let ids = Set(sessions.map(\.id))
        for session in sessions where firstSeen[session.id] == nil {
            firstSeen[session.id] = session.canvas?.newest?.changed ?? 0
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
        closed = closed.filter { ids.contains($0.key) }
        picks = picks.filter { ids.contains($0.key) }
        lastFiles = lastFiles.filter { ids.contains($0.key) }
        shelves.formIntersection(ids)
        let browsing = Set(sessions.filter { $0.canvas?.browser != nil }.map(\.id))
        feeds = feeds.filter { browsing.contains($0.key) }
        for key in stored.keys where !ids.contains(key.session) {
            forget(key)
        }
    }

    public func isNew(_ shown: CanvasShown, in session: String) -> Bool {
        shown.changed > (marks[session]?[shown.key] ?? firstSeen[session] ?? 0)
    }

    public func hasNews(_ session: RuntimeSession) -> Bool {
        session.canvas?.shown.contains { isNew($0, in: session.id) } ?? false
    }

    public func saw(_ shown: CanvasShown, in session: String) {
        guard isNew(shown, in: session) else { return }
        marks[session, default: [:]][shown.key] = shown.changed
    }

    /// What an open canvas shows: the item that the person picked, or else the newest, in the
    /// parts that are open.
    public func current(_ session: RuntimeSession) -> CanvasShown? {
        guard let canvas = session.canvas else { return nil }
        let open = sections(session)
        if let pick = picks[session.id].flatMap(canvas.shown), open.contains(pick.section) {
            return pick
        }
        return canvas.shown.filter { open.contains($0.section) }.max { $0.changed < $1.changed }
    }

    /// The parts of a canvas that are open on this device.
    public func sections(_ session: RuntimeSession) -> [CanvasSection] {
        guard let canvas = session.canvas else { return [] }
        return canvas.sections.filter { section in
            closed[session.id]?[section].map { (canvas.changed(of: section) ?? 0) > $0 } ?? true
        }
    }

    /// The parts that a person can open on a canvas: the ones they closed, and a browser.
    public func closedSections(_ session: RuntimeSession) -> [CanvasSection] {
        let open = sections(session)
        return [CanvasSection.files, .browser].filter { section in
            !open.contains(section) && (section == .browser || session.canvas?.sections.contains(section) == true)
        }
    }

    /// Closes a part of a canvas on this device until it changes.
    public func close(_ section: CanvasSection, in session: RuntimeSession) {
        guard let canvas = session.canvas else { return }
        closed[session.id, default: [:]][section] = canvas.changed(of: section) ?? 0
        if picks[session.id].flatMap(canvas.shown)?.section == section {
            picks[session.id] = nil
        }
        if section == .files {
            shelves.remove(session.id)
        }
    }

    /// The pictures and input of a terminal's browser on this device.
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

    /// Shows a part of a canvas, which opens if it was closed: the browser, or the file that the
    /// person saw last there, else the newest file.
    public func show(_ section: CanvasSection, in session: RuntimeSession) {
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

    /// Whether the canvas of a terminal shows all its files as cards.
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

    /// The content of an item that this device already has, for its card.
    public func stored(_ item: CanvasItem, in session: String) -> Data? {
        stored[Stored(session: session, key: item.key, revision: item.revision)]
    }

    /// How much of an item's content came, from 0 to 1, while this device reads it.
    public func read(_ key: String, in session: String) -> Double? {
        reads[Self.place(session, key)]
    }

    private static func place(_ session: String, _ key: String) -> String {
        session + "\u{1F}" + key
    }

    /// The bytes of an item and their revision, which can be newer than the item's.
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

    /// The next picture of the browser after `sequence`, or none when the browser shows no change
    /// for a while.
    public func picture(in session: String, after sequence: UInt64) async throws -> BrowserFrame? {
        let answer = try await ask(session, .object(["type": .string("picture"), "after": .uint(sequence)]))
        guard answer.answer == "picture", let sequence = answer.sequence else { throw CanvasError.unreadable }
        guard let frame = answer.frame else { return nil }
        guard let jpeg = Data(base64Encoded: frame.data) else { throw CanvasError.unreadable }
        return BrowserFrame(sequence: sequence, width: frame.width, height: frame.height, jpeg: jpeg)
    }

    /// Sends a person's input with the size of the view where they use the browser.
    public func send(_ events: [BrowserInput], width: Int, height: Int, in session: String) async throws {
        let events = try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(events))
        _ = try await ask(session, .object([
            "type": .string("input"), "events": events, "width": .int(Int64(width)), "height": .int(Int64(height)),
        ]))
    }

    /// Whether a person on this device opens the browser of a terminal, which did not come yet.
    public func opensBrowser(_ session: String) -> Bool {
        opening.contains(session)
    }

    /// Opens the shared browser of a terminal, which then shows on its canvas.
    public func openBrowser(in session: String) async throws {
        closed[session]?[.browser] = nil
        opening.insert(session)
        do {
            try await act(.wake(width: 0, height: 0), in: session)
        } catch {
            opening.remove(session)
            throw error
        }
    }

    public func act(_ act: BrowserAct, in session: String) async throws {
        _ = try await ask(session, .object(["type": .string("act"), "action": act.json]))
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
