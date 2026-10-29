import Foundation

public struct Canvas: Decodable, Equatable, Sendable {
    public let sequence: UInt64
    public let items: [CanvasItem]
    public let browser: CanvasBrowser?

    public init(sequence: UInt64, items: [CanvasItem], browser: CanvasBrowser?) {
        self.sequence = sequence
        self.items = items
        self.browser = browser
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        sequence = try container.decode(UInt64.self, forKey: .sequence)
        items = try container.decodeIfPresent([CanvasItem].self, forKey: .items) ?? []
        browser = try container.decodeIfPresent(CanvasBrowser.self, forKey: .browser)
    }

    private enum CodingKeys: String, CodingKey { case sequence, items, browser }

    /// The items, the newest first, and then the browser.
    public var shown: [CanvasShown] {
        items.map(CanvasShown.item) + (browser.map { [CanvasShown.browser($0)] } ?? [])
    }

    /// The item or browser with the newest change.
    public var newest: CanvasShown? {
        shown.max { $0.changed < $1.changed }
    }

    public func shown(_ key: String) -> CanvasShown? {
        shown.first { $0.key == key }
    }
}

public enum CanvasShown: Equatable, Identifiable, Sendable {
    case item(CanvasItem)
    case browser(CanvasBrowser)

    public static let browserKey = "browser"

    public var key: String {
        switch self {
        case let .item(item): item.key
        case .browser: Self.browserKey
        }
    }

    public var id: String {
        key
    }

    public var changed: UInt64 {
        switch self {
        case let .item(item): item.changed
        case let .browser(browser): browser.changed
        }
    }
}

public struct CanvasItem: Decodable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Decodable, Sendable { case page, picture, pdf, office }

    public let key: String
    public let name: String
    public let kind: Kind
    public let revision: UInt64
    public let bytes: UInt64
    public let changed: UInt64
    public let missing: Bool

    public var id: String {
        key
    }

    public init(key: String, name: String, kind: Kind, revision: UInt64, bytes: UInt64, changed: UInt64, missing: Bool = false) {
        self.key = key
        self.name = name
        self.kind = kind
        self.revision = revision
        self.bytes = bytes
        self.changed = changed
        self.missing = missing
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        key = try container.decode(String.self, forKey: .key)
        name = try container.decode(String.self, forKey: .name)
        kind = try container.decode(Kind.self, forKey: .kind)
        revision = try container.decode(UInt64.self, forKey: .revision)
        bytes = try container.decode(UInt64.self, forKey: .bytes)
        changed = try container.decode(UInt64.self, forKey: .changed)
        missing = try container.decodeIfPresent(Bool.self, forKey: .missing) ?? false
    }

    private enum CodingKeys: String, CodingKey { case key, name, kind, revision, bytes, changed, missing }
}

public struct CanvasBrowser: Decodable, Equatable, Sendable {
    public enum State: String, Decodable, Sendable { case starting, awake, asleep }

    public let state: State
    public let changed: UInt64
    public let tabs: [BrowserTab]
    public let width: Int
    public let height: Int
    public let last: BrowserAction?
    public let problem: String?

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        state = try container.decode(State.self, forKey: .state)
        changed = try container.decode(UInt64.self, forKey: .changed)
        tabs = try container.decodeIfPresent([BrowserTab].self, forKey: .tabs) ?? []
        width = try container.decode(Int.self, forKey: .width)
        height = try container.decode(Int.self, forKey: .height)
        last = try container.decodeIfPresent(BrowserAction.self, forKey: .last)
        problem = try container.decodeIfPresent(String.self, forKey: .problem)
    }

    private enum CodingKeys: String, CodingKey { case state, changed, tabs, width, height, last, problem }

    public var activeTab: BrowserTab? {
        tabs.first(where: \.active)
    }
}

public struct BrowserTab: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    public let title: String
    public let url: String
    public let active: Bool
}

public struct BrowserAction: Decodable, Equatable, Sendable {
    public let agent: String
    public let words: String
}

/// One picture of the shared browser, a JPEG at the browser's size in CSS pixels.
public struct BrowserFrame: Equatable, Sendable {
    public let sequence: UInt64
    public let width: Int
    public let height: Int
    public let jpeg: Data
}

public enum CanvasError: LocalizedError, Equatable {
    case unreadable

    public var errorDescription: String? {
        String(localized: "Kodosi got a canvas answer it cannot read.", bundle: .module)
    }
}

/// A person's pointer, key or text in the shared browser, in the browser's CSS pixels.
public enum BrowserInput: Encodable, Equatable, Sendable {
    public enum MouseEvent: String, Encodable, Sendable { case mousePressed, mouseReleased, mouseMoved, mouseWheel }
    public enum MouseButton: String, Encodable, Sendable { case none, left, middle, right }
    public enum KeyEvent: String, Encodable, Sendable { case keyDown, keyUp, rawKeyDown, char }
    public enum TouchEvent: String, Encodable, Sendable { case touchStart, touchMove, touchEnd, touchCancel }

    public struct Modifiers: OptionSet, Encodable, Sendable {
        public let rawValue: UInt8
        public init(rawValue: UInt8) {
            self.rawValue = rawValue
        }

        public static let alt = Modifiers(rawValue: 1)
        public static let control = Modifiers(rawValue: 2)
        public static let meta = Modifiers(rawValue: 4)
        public static let shift = Modifiers(rawValue: 8)
    }

    public struct Point: Encodable, Equatable, Sendable {
        public let x: Int
        public let y: Int

        public init(x: Int, y: Int) {
            self.x = x
            self.y = y
        }
    }

    case mouse(MouseEvent, x: Int, y: Int, button: MouseButton = .none, clicks: Int = 0, deltaX: Int = 0, deltaY: Int = 0,
               modifiers: Modifiers = [])
    case key(KeyEvent, key: String, code: String, text: String? = nil, keyCode: Int = 0, modifiers: Modifiers = [])
    case touch(TouchEvent, points: [Point], modifiers: Modifiers = [])
    case text(String)

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .mouse(event, x, y, button, clicks, deltaX, deltaY, modifiers):
            try container.encode("mouse", forKey: .type)
            try container.encode(event, forKey: .eventType)
            try container.encode(x, forKey: .x)
            try container.encode(y, forKey: .y)
            try container.encode(button, forKey: .button)
            try container.encode(clicks, forKey: .clickCount)
            try container.encode(deltaX, forKey: .deltaX)
            try container.encode(deltaY, forKey: .deltaY)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .key(event, key, code, text, keyCode, modifiers):
            try container.encode("key", forKey: .type)
            try container.encode(event, forKey: .eventType)
            try container.encode(key, forKey: .key)
            try container.encode(code, forKey: .code)
            try container.encodeIfPresent(text, forKey: .text)
            try container.encode(keyCode, forKey: .keyCode)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .touch(event, points, modifiers):
            try container.encode("touch", forKey: .type)
            try container.encode(event, forKey: .eventType)
            try container.encode(points, forKey: .points)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .text(text):
            try container.encode("text", forKey: .type)
            try container.encode(text, forKey: .text)
        }
    }

    private enum Field: String, CodingKey {
        case type, eventType, x, y, button, clickCount, deltaX, deltaY, modifiers, key, code, text, keyCode, points
    }
}

/// What a person does with the browser bar.
public enum BrowserAct: Equatable, Sendable {
    case open(String)
    case back, forward, reload
    case tab(String)
    case newTab
    case closeTab(String)
    case wake(width: Int, height: Int)

    var json: JSONValue {
        switch self {
        case let .open(url): .object(["do": .string("open"), "url": .string(url)])
        case .back: .object(["do": .string("back")])
        case .forward: .object(["do": .string("forward")])
        case .reload: .object(["do": .string("reload")])
        case let .tab(id): .object(["do": .string("tab"), "id": .string(id)])
        case .newTab: .object(["do": .string("newTab")])
        case let .closeTab(id): .object(["do": .string("closeTab"), "id": .string(id)])
        case let .wake(width, height): .object(["do": .string("wake"), "width": .int(Int64(width)), "height": .int(Int64(height))])
        }
    }
}
