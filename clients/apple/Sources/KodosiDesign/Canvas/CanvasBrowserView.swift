import KodosiBridge
import SwiftUI
import UniformTypeIdentifiers

#if os(iOS)
    private let control: CGFloat = 40
    private let closeKey: CGFloat = 22
#else
    private let control: CGFloat = 26
    private let closeKey: CGFloat = 16
#endif

/// The shared browser of a terminal: its tabs, the bar, and the picture that people click and
/// type in, with the agent's pointer over it. Asleep, the last picture stays in grey. The address
/// field offers the pages that this device's browsers showed, and words that are no address go
/// to the search engine.
public struct CanvasBrowserView: View {
    @Environment(\.theme) private var theme
    @AppStorage(BrowserHistory.storageKey) private var history = BrowserHistory()
    @Namespace private var thumb
    let browser: CanvasBrowser
    let feed: BrowserFeed
    var typing = false
    @Environment(\.displayScale) private var displayScale
    @State private var address = ""
    @State private var choice = 0
    @State private var picking = false
    @FocusState private var editing: Bool

    public init(browser: CanvasBrowser, feed: BrowserFeed, typing: Bool = false) {
        self.browser = browser
        self.feed = feed
        self.typing = typing
    }

    private var blank: Bool {
        (browser.activeTab?.url ?? Self.blankPage) == Self.blankPage
    }

    private static let blankPage = "about:blank"

    public var body: some View {
        VStack(spacing: 0) {
            if browser.state == .awake {
                tabStrip.transition(.opacity)
            }
            bar.zIndex(1)
            ZStack {
                theme.colors.terminal
                BrowserVideoView(feed: feed)
                    .opacity(feed.hasPicture && !blank ? 1 : 0)
                    .grayscale(browser.state == .awake ? 0 : 1)
                    .animation(theme.motion.fade, value: browser.state)
                if blank, browser.state == .awake {
                    Image(systemName: "globe").font(.system(size: 34, weight: .light))
                        .foregroundStyle(theme.colors.inkFaint.opacity(0.6))
                        .transition(.opacity)
                }
                if browser.state == .awake {
                    BrowserInputArea(feed: feed, typing: typing)
                    BrowserPointersLayer(feed: feed)
                    AgentPointerLayer(browser: browser, feed: feed)
                    BrowserPartsLayer(feed: feed)
                } else {
                    sleeping.transition(.opacity)
                }
            }
            .clipped()
            .onGeometryChange(for: CGSize.self) { $0.size } action: { feed.viewSize = $0 }
            .onChange(of: displayScale, initial: true) { _, scale in feed.viewScale = scale }
            .task(id: browser.problem == nil) {
                if browser.problem == nil {
                    await feed.watch()
                }
            }
            .onChange(of: feed.window.copied) { _, words in
                if let words {
                    Pasteboard.copy(words)
                }
            }
            .onChange(of: feed.window.chooser) { _, chooser in picking = chooser != nil }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.item], allowsMultipleSelection: feed.window.chooser?.several == true) { result in
                Task { await feed.give((try? result.get()) ?? []) }
            } onCancellation: {
                feed.send(.files([]))
            }
        }
        .animation(theme.motion.fade, value: browser.state)
        .onChange(of: browser.activeTab?.url, initial: true) { _, url in
            if !editing {
                address = url == Self.blankPage ? "" : url ?? ""
            }
        }
        .onChange(of: browser.tabs.map(TabPage.init)) { old, _ in
            let before = Set(old)
            for tab in browser.tabs where !before.contains(TabPage(tab)) {
                history.visit(tab.url, title: tab.title)
            }
        }
        .onChange(of: browser.tabs.map(\.title)) {
            for tab in browser.tabs {
                history.name(tab.url, title: tab.title)
            }
        }
    }

    /// The page of a tab. A page that a tab opens counts as a visit; a change of tab does not.
    private struct TabPage: Hashable {
        let tab: String
        let address: String

        init(_ tab: BrowserTab) {
            self.tab = tab.id
            address = tab.url
        }
    }

    /// The tabs of the window: as the picture's link tells them while it is open, and else as the
    /// terminal's canvas does.
    private var tabs: [BrowserWindow.Tab] {
        if !feed.window.tabs.isEmpty {
            return feed.window.tabs
        }
        return browser.tabs.map { BrowserWindow.Tab(id: $0.id, title: $0.title, address: $0.url, active: $0.active, loading: false) }
    }

    private var tabStrip: some View {
        let tabs = tabs
        let active = tabs.first(where: \.active)?.id
        let plus = IconButton(title: "New tab", bundle: .module, symbol: "plus", identifier: "canvas.browser.newTab", size: control) {
            feed.send(.newTab)
        }
        return ViewThatFits(in: .horizontal) {
            HStack(spacing: 4) {
                tabKeys(tabs)
                plus
                Spacer(minLength: 0)
            }
            HStack(spacing: 4) {
                ScrollViewReader { reader in
                    ScrollView(.horizontal) {
                        tabKeys(tabs)
                    }
                    .scrollIndicators(.never)
                    .onChange(of: active, initial: true) { _, active in
                        if let active {
                            withAnimation(theme.motion.snappy) { reader.scrollTo(active) }
                        }
                    }
                }
                plus
            }
        }
        .padding(.horizontal, 6).padding(.top, 4)
        .animation(theme.motion.snappy, value: tabs.map(\.id))
        .animation(theme.motion.snappy, value: active)
    }

    private func tabKeys(_ tabs: [BrowserWindow.Tab]) -> some View {
        HStack(spacing: 2) {
            ForEach(tabs) { tab in
                tabKey(tab, closes: tabs.count > 1)
                    .id(tab.id)
                    .transition(.scale(scale: 0.9).combined(with: .opacity))
            }
        }
        .padding(.vertical, 1)
    }

    private func tabKey(_ tab: BrowserWindow.Tab, closes: Bool) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.xs, style: .continuous)
        let title = Self.title(of: tab)
        return Button { feed.send(.tab(tab.id)) } label: {
            HStack(spacing: 6) {
                Group {
                    if tab.loading {
                        Spinner(size: 10)
                    } else {
                        Image(systemName: "globe").font(.system(size: 10, weight: .semibold))
                    }
                }
                .foregroundStyle(tab.active ? theme.colors.accentStrong : theme.colors.inkFaint)
                .frame(width: 12)
                Text(title).lineLimit(1).truncationMode(.tail)
            }
            .appTextStyle(.footnote).fontWeight(.medium)
            .foregroundStyle(tab.active ? theme.colors.ink : theme.colors.inkMuted)
            .padding(.leading, 8).padding(.trailing, closes ? closeKey + 10 : 10)
            .frame(height: control - 2).frame(minWidth: 80, maxWidth: 200, alignment: .leading)
            .background {
                if tab.active {
                    RaisedBackground(shape: shape, fill: theme.colors.raised, elevation: .resting)
                        .matchedGeometryEffect(id: "tab", in: thumb)
                }
            }
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .help(tab.address)
        .accessibilityLabel(Text(title))
        .accessibilityAddTraits(tab.active ? .isSelected : [])
        .accessibilityIdentifier("canvas.browser.tab.\(AccessibilityIdentifier.token(tab.id))")
        .overlay(alignment: .trailing) {
            if closes {
                RemoveKey(size: closeKey, title: String(localized: "Close tab", bundle: .module),
                          identifier: "canvas.browser.close.\(AccessibilityIdentifier.token(tab.id))") { feed.send(.closeTab(tab.id)) }
                    .padding(.trailing, 5)
            }
        }
    }

    private static func title(of tab: BrowserWindow.Tab) -> String {
        if tab.address.isEmpty || tab.address == blankPage {
            return String(localized: "New tab", bundle: .module)
        }
        if !tab.title.isEmpty, tab.title != tab.address {
            return tab.title
        }
        return URL(string: tab.address)?.host() ?? tab.address
    }

    private var bar: some View {
        HStack(spacing: 2) {
            HStack(spacing: 2) {
                IconButton(title: "Back", bundle: .module, symbol: "chevron.left", identifier: "canvas.browser.back", size: control) {
                    feed.send(.back)
                }
                IconButton(title: "Forward", bundle: .module, symbol: "chevron.right", identifier: "canvas.browser.forward", size: control) {
                    feed.send(.forward)
                }
                IconButton(title: "Reload", bundle: .module, symbol: "arrow.clockwise", identifier: "canvas.browser.reload", size: control) {
                    feed.send(.reload)
                }
                addressField
            }
            .disabled(browser.state != .awake)
            if let failure = feed.failure ?? browser.problem {
                Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(theme.colors.caution).help(failure)
                    .accessibilityLabel(Text(failure))
            }
        }
        .padding(.horizontal, 6).frame(height: control + 8)
    }

    private var addressField: some View {
        let suggestions = suggestions
        return HStack(spacing: 6) {
            Image(systemName: editing ? "magnifyingglass" : secure ? "lock.fill" : "globe").font(.system(size: 9.5, weight: .semibold))
                .foregroundStyle(theme.colors.inkFaint)
            TextField(String(localized: "Search or enter an address", bundle: .module), text: $address)
                .textFieldStyle(.plain).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                .focused($editing)
                .onSubmit { open(suggestions.indices.contains(choice) ? suggestions[choice] : nil) }
                .onKeyPress(.downArrow) { move(1, in: suggestions) }
                .onKeyPress(.upArrow) { move(-1, in: suggestions) }
                .onKeyPress(.escape) {
                    guard editing else { return .ignored }
                    editing = false
                    address = blank ? "" : browser.activeTab?.url ?? ""
                    return .handled
                }
                .onChange(of: address) { choice = 0 }
                .accessibilityIdentifier("canvas.browser.address")
        }
        .padding(.horizontal, 10).frame(height: control - 2).frame(maxWidth: 620).wellCapsule()
        .overlay(alignment: .topLeading) {
            if !suggestions.isEmpty {
                suggestionList(suggestions)
                    .offset(y: control + 2)
                    .transition(.opacity)
            }
        }
        .animation(theme.motion.fade, value: suggestions.isEmpty)
        .padding(.leading, 4)
        .layoutPriority(1)
    }

    private var secure: Bool {
        browser.activeTab?.url.hasPrefix("https://") == true
    }

    /// What Enter opens: the words as they are, then the pages that fit them.
    private enum Suggestion: Identifiable, Equatable {
        case words(String)
        case page(BrowserHistory.Page)

        var id: String {
            switch self {
            case let .words(words): "words:\(words)"
            case let .page(page): page.address
            }
        }
    }

    private var suggestions: [Suggestion] {
        let typed = address.trimmingCharacters(in: .whitespaces)
        guard editing, !typed.isEmpty, typed != browser.activeTab?.url else { return [] }
        return [.words(typed)] + history.matches(typed).filter { $0.address != typed }.map(Suggestion.page)
    }

    private func suggestionList(_ suggestions: [Suggestion]) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        return VStack(alignment: .leading, spacing: 1) {
            ForEach(Array(suggestions.enumerated()), id: \.element.id) { index, suggestion in
                Button { open(suggestion) } label: {
                    suggestionRow(suggestion).hoverRow(radius: Radius.sm, selected: index == choice)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("canvas.browser.suggestion.\(index)")
            }
        }
        .padding(4)
        .fixedSize(horizontal: false, vertical: true)
        .background { RaisedBackground(shape: shape, fill: theme.colors.raised, elevation: .lifted) }
    }

    private func suggestionRow(_ suggestion: Suggestion) -> some View {
        HStack(spacing: 8) {
            switch suggestion {
            case let .words(words):
                let searches = Self.searches(words)
                Image(systemName: searches ? "magnifyingglass" : "globe").font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(theme.colors.inkFaint).frame(width: 14)
                Text(words).foregroundStyle(theme.colors.ink).lineLimit(1).truncationMode(.middle)
                if searches {
                    Text("Search with Google", bundle: .module).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
                        .layoutPriority(-1)
                }
            case let .page(page):
                let bare = BrowserHistory.bare(page.address)
                Image(systemName: "clock.arrow.circlepath").font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(theme.colors.inkFaint).frame(width: 14)
                Text(page.title.isEmpty ? bare : page.title).foregroundStyle(theme.colors.ink).lineLimit(1)
                Text(bare).foregroundStyle(theme.colors.inkFaint).lineLimit(1).truncationMode(.middle).layoutPriority(-1)
            }
            Spacer(minLength: 0)
        }
        .appTextStyle(.footnote)
        .padding(.horizontal, 8).frame(height: control)
        .contentShape(Rectangle())
    }

    /// Whether the host takes the words as a search, by the rule of its address bar: they hold a
    /// space or an `@`, or no computer's name or address comes before their path.
    private static func searches(_ words: String) -> Bool {
        if words.contains("://") || words.hasPrefix("about:") || words.hasPrefix("data:") {
            return false
        }
        let host = words.prefix { !"/?#".contains($0) }
        if words.contains(where: \.isWhitespace) || host.isEmpty || host.contains("@") {
            return true
        }
        if host.hasPrefix("[") {
            return false
        }
        var name = host
        var port = false
        if let colon = host.lastIndex(of: ":") {
            let digits = host[host.index(after: colon)...]
            guard digits.allSatisfy(\.isASCIIDigit) else { return true }
            name = host[..<colon]
            port = !digits.isEmpty
        }
        let labels = name.lowercased().split(separator: ".", omittingEmptySubsequences: false)
        let last = labels.last ?? ""
        let address = labels.count == 4 && labels.allSatisfy { label in
            (1 ... 3).contains(label.count) && label.allSatisfy(\.isASCIIDigit) && (label.count == 1 || label.first != "0")
                && (Int(label) ?? 256) <= 255
        }
        let local = last == "localhost" || last == "local" || address || (labels.count == 1 && port)
        let domain = labels.count > 1 && last.utf8.count >= 2 && last.allSatisfy(\.isLetter)
        return !local && !domain
    }

    private func move(_ step: Int, in suggestions: [Suggestion]) -> KeyPress.Result {
        guard !suggestions.isEmpty else { return .ignored }
        choice = (choice + step + suggestions.count) % suggestions.count
        return .handled
    }

    @ViewBuilder
    private var sleeping: some View {
        switch browser.state {
        case .starting:
            VStack(spacing: 10) {
                Spinner(size: 18)
                if let getting = browser.getting {
                    Text("Getting the browser, \(getting)%", bundle: .module)
                        .appTextStyle(.footnote).monospacedDigit().foregroundStyle(theme.colors.inkMuted)
                        .contentTransition(.numericText())
                }
            }
        case .asleep:
            Button { Task { await feed.wake() } } label: {
                HStack(spacing: 7) {
                    Image(systemName: "moon.zzz.fill").font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(theme.colors.inkMuted)
                    Text("Wake", bundle: .module).appTextStyle(.footnote).fontWeight(.semibold).foregroundStyle(theme.colors.ink)
                }
                .padding(.horizontal, 14).frame(height: 32)
                .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
                .contentShape(Capsule())
            }
            .buttonStyle(PressScaleStyle())
            .help(browser.problem ?? String(localized: "The browser is asleep. It opens its last page again.", bundle: .module))
            .accessibilityIdentifier("canvas.browser.wake")
        case .awake:
            EmptyView()
        }
    }

    private func open(_ suggestion: Suggestion?) {
        let target = switch suggestion {
        case let .page(page): page.address
        case let .words(words): words
        case nil: address.trimmingCharacters(in: .whitespaces)
        }
        guard !target.isEmpty else { return }
        editing = false
        address = target
        feed.send(.open(target))
    }
}

private extension Character {
    var isASCIIDigit: Bool {
        isASCII && isWholeNumber
    }
}
