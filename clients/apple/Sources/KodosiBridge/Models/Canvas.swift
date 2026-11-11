import CoreGraphics
import Foundation

public struct Canvas: Decodable, Equatable, Sendable {
    public let sequence: UInt64
    public let items: [CanvasItem]
    public let browser: CanvasBrowser?
    /// What the agent showed last, which each device acts on once.
    public let open: CanvasOpen?
    /// What changed last: a file, or the browser by a command of the agent. Each device acts on it once.
    public let change: CanvasOpen?

    public init(sequence: UInt64, items: [CanvasItem], browser: CanvasBrowser?, open: CanvasOpen? = nil, change: CanvasOpen? = nil) {
        self.sequence = sequence
        self.items = items
        self.browser = browser
        self.open = open
        self.change = change
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        sequence = try container.decode(UInt64.self, forKey: .sequence)
        items = try container.decodeIfPresent([CanvasItem].self, forKey: .items) ?? []
        browser = try container.decodeIfPresent(CanvasBrowser.self, forKey: .browser)
        open = try container.decodeIfPresent(CanvasOpen.self, forKey: .open)
        change = try container.decodeIfPresent(CanvasOpen.self, forKey: .change)
    }

    private enum CodingKeys: String, CodingKey { case sequence, items, browser, open, change }

    /// The items, the newest first, and then the browser.
    public var shown: [CanvasShown] {
        items.map(CanvasShown.item) + (browser.map { [CanvasShown.browser($0)] } ?? [])
    }

    /// The item or browser with the newest change.
    public var newest: CanvasShown? {
        shown.max { $0.changed < $1.changed }
    }

    /// The files from the first shown to the last.
    public var files: [CanvasItem] {
        items.reversed()
    }

    /// The parts of the canvas: its files, and its browser.
    public var sections: [CanvasSection] {
        (items.isEmpty ? [] : [.files]) + (browser == nil ? [] : [.browser])
    }

    /// The newest change in a part of the canvas.
    public func changed(of section: CanvasSection) -> UInt64? {
        switch section {
        case .files: items.map(\.changed).max()
        case .browser: browser?.changed
        }
    }

    public func shown(_ key: String) -> CanvasShown? {
        shown.first { $0.key == key }
    }
}

public enum CanvasSection: String, Identifiable, Sendable {
    case files, browser

    public var id: String {
        rawValue
    }
}

/// The item or browser that the agent showed, by its key, and the stamp of the canvas then.
public struct CanvasOpen: Decodable, Equatable, Sendable {
    public let key: String
    public let at: UInt64

    public init(key: String, at: UInt64) {
        self.key = key
        self.at = at
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

    public var section: CanvasSection {
        if case .browser = self {
            return .browser
        }
        return .files
    }
}

public struct CanvasItem: Decodable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Decodable, Sendable { case page, picture, pdf, document, sheet, presentation }
    /// The live view of a file that people edit together.
    public enum View: String, Decodable, Sendable { case docx, xlsx, pptx, pdf, text }

    public let key: String
    public let name: String
    public let kind: Kind
    public let view: View?
    public let revision: UInt64
    public let bytes: UInt64
    public let changed: UInt64
    public let missing: Bool

    public var id: String {
        key
    }

    public init(key: String, name: String, kind: Kind, view: View? = nil, revision: UInt64, bytes: UInt64, changed: UInt64,
                missing: Bool = false)
    {
        self.key = key
        self.name = name
        self.kind = kind
        self.view = view
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
        view = try container.decodeIfPresent(View.self, forKey: .view)
        revision = try container.decode(UInt64.self, forKey: .revision)
        bytes = try container.decode(UInt64.self, forKey: .bytes)
        changed = try container.decode(UInt64.self, forKey: .changed)
        missing = try container.decodeIfPresent(Bool.self, forKey: .missing) ?? false
    }

    private enum CodingKeys: String, CodingKey { case key, name, kind, view, revision, bytes, changed, missing }
}

public struct CanvasBrowser: Decodable, Equatable, Sendable {
    public enum State: String, Decodable, Sendable { case starting, awake, asleep }

    public let state: State
    public let changed: UInt64
    public let tabs: [BrowserTab]
    public let width: Int
    public let height: Int
    /// The part of the first download of the browser, from 0 to 100.
    public let getting: Int?
    public let doing: BrowserDoing?
    public let last: BrowserAction?
    public let problem: String?

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        state = try container.decode(State.self, forKey: .state)
        changed = try container.decode(UInt64.self, forKey: .changed)
        tabs = try container.decodeIfPresent([BrowserTab].self, forKey: .tabs) ?? []
        width = try container.decode(Int.self, forKey: .width)
        height = try container.decode(Int.self, forKey: .height)
        getting = try container.decodeIfPresent(Int.self, forKey: .getting)
        doing = try container.decodeIfPresent(BrowserDoing.self, forKey: .doing)
        last = try container.decodeIfPresent(BrowserAction.self, forKey: .last)
        problem = try container.decodeIfPresent(String.self, forKey: .problem)
    }

    private enum CodingKeys: String, CodingKey { case state, changed, tabs, width, height, getting, doing, last, problem }

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

/// What the agent does in the browser while its command runs, and the middle of the element that
/// it acts on, in the browser's CSS pixels.
public struct BrowserDoing: Decodable, Equatable, Sendable {
    public enum Kind: String, Decodable, Sendable { case open, click, type, scroll, read, wait, other }

    public struct Point: Decodable, Equatable, Sendable {
        public let x: Int
        public let y: Int

        public init(x: Int, y: Int) {
            self.x = x
            self.y = y
        }
    }

    public let agent: String
    public let kind: Kind
    public let point: Point?

    public init(agent: String, kind: Kind, point: Point?) {
        self.agent = agent
        self.kind = kind
        self.point = point
    }
}

/// What Settings › Browser shows of this computer.
public struct BrowserStatus: Equatable, Sendable {
    /// A browser profile whose sign-ins a person can copy.
    public struct Source: Decodable, Hashable, Sendable {
        public let name: String
        /// The browser, such as `chrome` or `safari`.
        public let browser: String
        /// macOS keeps the data of the browser from Kodosi until the person gives Kodosi Full Disk
        /// Access.
        public let protected: Bool

        public init(name: String, browser: String, protected: Bool) {
            self.name = name
            self.browser = browser
            self.protected = protected
        }
    }

    /// The browsers whose sign-ins a person can copy.
    public let sources: [Source]
    /// For how many sites copies gave sign-ins.
    public let sites: Int
    /// The Kodosi Browser of this computer, when it has one.
    public let engine: String?
    /// The part of a download of Kodosi Browser, from 0 to 100, while one runs.
    public let getting: Int?

    public init(sources: [Source], sites: Int, engine: String?, getting: Int?) {
        self.sources = sources
        self.sites = sites
        self.engine = engine
        self.getting = getting
    }
}

public enum CanvasError: LocalizedError, Equatable {
    case unreadable

    public var errorDescription: String? {
        String(localized: "Kodosi got a canvas answer it cannot read.", bundle: .module)
    }
}

/// What a window tells a viewer around the picture.
public struct BrowserWindow: Decodable, Equatable, Sendable {
    public struct Pointer: Decodable, Equatable, Identifiable, Sendable {
        public let viewer: String
        public let person: String
        public let name: String
        public let x: Double
        public let y: Double

        public var id: String {
            viewer
        }
    }

    public struct MenuItem: Decodable, Equatable, Sendable {
        public let label: String
        public let enabled: Bool
        public let chosen: Bool
    }

    public struct Menu: Decodable, Equatable, Sendable {
        public let id: Int
        public let items: [MenuItem]
    }

    public enum DialogKind: String, Decodable, Sendable { case alert, confirm, prompt, beforeunload }

    public struct Dialog: Decodable, Equatable, Sendable {
        public let kind: DialogKind
        public let message: String
        public let text: String
    }

    public struct Chooser: Decodable, Equatable, Sendable {
        public let several: Bool
    }

    /// A tab of the window, as the viewer sees it now.
    public struct Tab: Decodable, Equatable, Identifiable, Sendable {
        public let id: String
        public let title: String
        public let address: String
        public let active: Bool
        public let loading: Bool

        public init(id: String, title: String, address: String, active: Bool, loading: Bool) {
            self.id = id
            self.title = title
            self.address = address
            self.active = active
            self.loading = loading
        }
    }

    public var tabs: [Tab] = []
    public var width = 0
    public var height = 0
    public var shape = ""
    public var pointers: [Pointer] = []
    public var menu: Menu?
    public var dialog: Dialog?
    public var chooser: Chooser?
    public var copied: String?

    public var size: CGSize {
        CGSize(width: width, height: height)
    }

    public init() {}

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        tabs = try container.decodeIfPresent([Tab].self, forKey: .tabs) ?? []
        width = try container.decode(Int.self, forKey: .width)
        height = try container.decode(Int.self, forKey: .height)
        shape = try container.decode(String.self, forKey: .shape)
        pointers = try container.decode([Pointer].self, forKey: .pointers)
        menu = try container.decodeIfPresent(Menu.self, forKey: .menu)
        dialog = try container.decodeIfPresent(Dialog.self, forKey: .dialog)
        chooser = try container.decodeIfPresent(Chooser.self, forKey: .chooser)
        copied = try container.decodeIfPresent(String.self, forKey: .copied)
    }

    private enum CodingKeys: String, CodingKey { case tabs, width, height, shape, pointers, menu, dialog, chooser, copied }
}

/// A person's step in the shared browser, in the CSS pixels of its page.
public enum BrowserInput: Encodable, Equatable, Sendable {
    public enum MouseEvent: String, Encodable, Sendable {
        case mousePressed = "press", mouseReleased = "release", mouseMoved = "move", mouseWheel = "wheel"
    }

    public enum MouseButton: String, Encodable, Sendable { case none, left, middle, right }
    public enum KeyEvent: String, Encodable, Sendable { case keyDown = "down", keyUp = "up" }

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
        public let x: Double
        public let y: Double

        public init(x: Double, y: Double) {
            self.x = x
            self.y = y
        }
    }

    case mouse(MouseEvent, x: Double, y: Double, button: MouseButton = .none, clicks: Int = 0, deltaX: Double = 0, deltaY: Double = 0,
               modifiers: Modifiers = [])
    case key(KeyEvent, key: String, code: String, text: String? = nil, keyCode: Int = 0, modifiers: Modifiers = [])
    case text(String)
    case open(String)
    case back, forward, reload
    case tab(String)
    case newTab
    case closeTab(String)
    case choose(menu: Int, choice: Int?)
    case dialog(accept: Bool, text: String?)
    case files([String])

    var isPlainMove: Bool {
        if case .mouse(.mouseMoved, _, _, .none, _, _, _, _) = self {
            return true
        }
        return false
    }

    // swiftlint:disable:next cyclomatic_complexity
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .mouse(event, x, y, button, clicks, deltaX, deltaY, modifiers):
            try container.encode("mouse", forKey: .kind)
            try container.encode(event, forKey: .action)
            try container.encode(x, forKey: .x)
            try container.encode(y, forKey: .y)
            try container.encode(button, forKey: .button)
            try container.encode(clicks, forKey: .clicks)
            try container.encode(deltaX, forKey: .deltaX)
            try container.encode(deltaY, forKey: .deltaY)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .key(event, key, code, text, keyCode, modifiers):
            try container.encode("key", forKey: .kind)
            try container.encode(event, forKey: .action)
            try container.encode(key, forKey: .key)
            try container.encode(code, forKey: .code)
            try container.encodeIfPresent(text, forKey: .text)
            try container.encode(keyCode, forKey: .keyCode)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .text(text):
            try container.encode("text", forKey: .kind)
            try container.encode(text, forKey: .text)
        case let .open(address):
            try container.encode("open", forKey: .kind)
            try container.encode(address, forKey: .address)
        case .back:
            try container.encode("back", forKey: .kind)
        case .forward:
            try container.encode("forward", forKey: .kind)
        case .reload:
            try container.encode("reload", forKey: .kind)
        case let .tab(id):
            try container.encode("tab", forKey: .kind)
            try container.encode(id, forKey: .id)
        case .newTab:
            try container.encode("newTab", forKey: .kind)
        case let .closeTab(id):
            try container.encode("closeTab", forKey: .kind)
            try container.encode(id, forKey: .id)
        case let .choose(menu, choice):
            try container.encode("choose", forKey: .kind)
            try container.encode(menu, forKey: .menu)
            try container.encodeIfPresent(choice, forKey: .choice)
        case let .dialog(accept, text):
            try container.encode("dialog", forKey: .kind)
            try container.encode(accept, forKey: .accept)
            try container.encodeIfPresent(text, forKey: .text)
        case let .files(paths):
            try container.encode("files", forKey: .kind)
            try container.encode(paths, forKey: .paths)
        }
    }

    private enum Field: String, CodingKey {
        case kind, action, x, y, button, clicks, deltaX, deltaY, modifiers, key, code, text, keyCode, address, id, menu, choice, accept, paths
    }
}

/// A person's step that needs no view of the browser.
public enum BrowserAct: Equatable, Sendable {
    case wake

    var json: JSONValue {
        switch self {
        case .wake: .object(["do": .string("wake")])
        }
    }
}
