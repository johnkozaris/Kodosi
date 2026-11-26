import CoreGraphics
import Foundation

/// The screen stays on the canvas while the person at its computer does not share it, so that the
/// owner's devices say why it does not show and watch it again once it is shared.
public struct CanvasScreen: Decodable, Equatable, Sendable {
    public let shared: Bool
}

public struct Canvas: Decodable, Equatable, Sendable {
    public let items: [CanvasItem]
    public let browser: CanvasBrowser?
    /// The screen of the terminal's computer, which only its owner's devices see.
    public let screen: CanvasScreen?
    /// Most recent item the agent explicitly opened; each device handles it once.
    public let open: CanvasOpen?
    /// Most recent file or agent-driven browser change; each device handles it once.
    public let change: CanvasOpen?

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        items = try container.decodeIfPresent([CanvasItem].self, forKey: .items) ?? []
        browser = try container.decodeIfPresent(CanvasBrowser.self, forKey: .browser)
        screen = try container.decodeIfPresent(CanvasScreen.self, forKey: .screen)
        open = try container.decodeIfPresent(CanvasOpen.self, forKey: .open)
        change = try container.decodeIfPresent(CanvasOpen.self, forKey: .change)
    }

    private enum CodingKeys: String, CodingKey { case items, browser, screen, open, change }

    public var shown: [CanvasShown] {
        items.map(CanvasShown.item) + (browser.map { [CanvasShown.browser($0)] } ?? []) + (screen == nil ? [] : [.screen])
    }

    public var newest: CanvasShown? {
        shown.max { $0.changed < $1.changed }
    }

    public var files: [CanvasItem] {
        items.reversed()
    }

    public var sections: [CanvasSection] {
        (items.isEmpty ? [] : [.files]) + (browser == nil ? [] : [.browser]) + (screen == nil ? [] : [.screen])
    }

    public func changed(of section: CanvasSection) -> UInt64? {
        switch section {
        case .files: items.map(\.changed).max()
        case .browser: browser?.changed
        case .screen: screen == nil ? nil : CanvasShown.screen.changed
        }
    }

    public func shown(_ key: String) -> CanvasShown? {
        shown.first { $0.key == key }
    }
}

public enum CanvasSection: String, Identifiable, Sendable {
    case files, browser, screen

    public var id: String {
        rawValue
    }
}

public struct CanvasOpen: Decodable, Equatable, Sendable {
    public let key: String
    public let at: UInt64
}

public enum CanvasShown: Equatable, Identifiable, Sendable {
    case item(CanvasItem)
    case browser(CanvasBrowser)
    case screen

    public static let browserKey = "browser"
    public static let screenKey = "screen"

    public var key: String {
        switch self {
        case let .item(item): item.key
        case .browser: Self.browserKey
        case .screen: Self.screenKey
        }
    }

    public var id: String {
        key
    }

    /// The screen does not change as a file or the browser does: it shows itself when an agent
    /// or a person shows it.
    public var changed: UInt64 {
        switch self {
        case let .item(item): item.changed
        case let .browser(browser): browser.changed
        case .screen: 0
        }
    }

    public var section: CanvasSection {
        switch self {
        case .item: .files
        case .browser: .browser
        case .screen: .screen
        }
    }
}

public struct CanvasItem: Decodable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Decodable, Sendable { case page, picture, pdf, document, sheet, presentation }
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
    /// Initial browser download progress (0...100) while downloading.
    public let getting: Int?
    public let doing: BrowserDoing?
    public let problem: String?
    /// True for shared terminals, whose browser uses an isolated sign-in profile.
    public let ownSignIns: Bool

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        state = try container.decode(State.self, forKey: .state)
        changed = try container.decode(UInt64.self, forKey: .changed)
        tabs = try container.decodeIfPresent([BrowserTab].self, forKey: .tabs) ?? []
        getting = try container.decodeIfPresent(Int.self, forKey: .getting)
        doing = try container.decodeIfPresent(BrowserDoing.self, forKey: .doing)
        problem = try container.decodeIfPresent(String.self, forKey: .problem)
        ownSignIns = try container.decodeIfPresent(Bool.self, forKey: .ownSignIns) ?? false
    }

    private enum CodingKeys: String, CodingKey { case state, changed, tabs, getting, doing, problem, ownSignIns }

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

/// `point` is the center of the element the agent is interacting with, in browser CSS pixels.
public struct BrowserDoing: Decodable, Equatable, Sendable {
    public enum Kind: String, Decodable, Sendable { case open, click, type, scroll, read, wait, other }

    public struct Point: Decodable, Equatable, Sendable {
        public let x: Int
        public let y: Int
    }

    public let agent: String
    public let kind: Kind
    public let point: Point?
}

public struct BrowserStatus: Equatable, Sendable {
    public struct Source: Decodable, Hashable, Sendable {
        public let name: String
        public let browser: String
        /// True when macOS blocks reading this browser's profile until Full Disk Access is granted.
        public let protected: Bool

        public init(name: String, browser: String, protected: Bool) {
            self.name = name
            self.browser = browser
            self.protected = protected
        }
    }

    public let sources: [Source]
    public let sites: Int
    public let engine: String?
    /// Kodosi Browser download progress (0...100) while a download is active.
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
        /// Menu anchor position in page CSS pixels.
        public let x: Double
        public let y: Double
        public let items: [MenuItem]
        public let context: MenuContext?
    }

    /// Context for a right-click; the client builds the context menu items from these fields.
    public struct MenuContext: Decodable, Equatable, Sendable {
        public static let copySelection = 0
        public static let copyLink = 1

        public let selection: String
        public let link: String?

        public var words: String? {
            selection.isEmpty ? link : selection
        }
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
    /// Changed tab favicons by tab ID (base64 PNG); an empty string clears a tab's icon.
    public var icons: [String: String] = [:]
    /// Core Animation `CALayerHost` context ID used instead of video when viewing on the host Mac.
    public var layer: UInt32?

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
        icons = try container.decodeIfPresent([String: String].self, forKey: .icons) ?? [:]
        layer = try container.decodeIfPresent(UInt32.self, forKey: .layer)
    }

    private enum CodingKeys: String, CodingKey { case tabs, width, height, shape, pointers, menu, dialog, chooser, copied, icons, layer }
}

/// What a viewer does in the shared browser: its pointer and keys, and the browser's own commands.
public enum BrowserInput: Encodable, Equatable, Sendable {
    case viewer(ViewerInput)
    case open(String)
    case back, forward, reload
    case tab(String)
    case newTab
    case closeTab(String)
    case choose(menu: Int, choice: Int?)
    case dialog(accept: Bool, text: String?)
    case files([String])

    // swiftlint:disable:next cyclomatic_complexity
    public func encode(to encoder: any Encoder) throws {
        if case let .viewer(input) = self {
            try input.encode(to: encoder)
            return
        }
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case .viewer:
            break
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
        case kind, text, address, id, menu, choice, accept, paths
    }
}

public enum BrowserAct: Equatable, Sendable {
    case wake

    var json: JSONValue {
        switch self {
        case .wake: .object(["do": .string("wake")])
        }
    }
}
