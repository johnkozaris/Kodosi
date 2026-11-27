import KodosiBridge
import SwiftUI

private let control = CanvasBar.key

/// The screen of the terminal's computer, which its owner watches and controls from their other
/// devices. The computer itself only says that the screen is on its canvas, and whether it shares it.
public struct CanvasScreenView: View {
    let session: RuntimeSession
    let feed: ScreenFeed
    var typing = false

    public init(session: RuntimeSession, feed: ScreenFeed, typing: Bool = false) {
        self.session = session
        self.feed = feed
        self.typing = typing
    }

    public var body: some View {
        if session.kind == .local {
            ThisScreen(shared: session.canvas?.screen?.shared ?? false, feed: feed)
        } else {
            VStack(spacing: 0) {
                ScreenBar(feed: feed).zIndex(1)
                if feed.access?.input == false, [.live, .resting].contains(feed.showing) {
                    UncontrolledLine(computer: session.hostLabel)
                }
                ScreenStage(computer: session.hostLabel, feed: feed, typing: typing)
            }
        }
    }
}

private struct ThisScreen: View {
    let shared: Bool
    let feed: ScreenFeed
    @State private var turning = false

    var body: some View {
        TerminalStateCard(symbol: "display", title: String(localized: "This Mac's screen", bundle: .module),
                          message: shared ? String(localized: "Your other devices show it on this canvas.", bundle: .module)
                              : String(localized: "Sharing is off", bundle: .module), working: false)
        {
            if !shared {
                Button {
                    turning = true
                    Task {
                        await feed.share()
                        turning = false
                    }
                } label: {
                    Text("Turn on", bundle: .module)
                }
                .buttonStyle(.kodosi(.secondary))
                .disabled(turning)
                .accessibilityIdentifier("canvas.screen.turnOn")
            }
        }
        .accessibilityIdentifier("canvas.screen.here")
    }
}

private struct ScreenStage: View {
    @Environment(\.theme) private var theme
    @Environment(\.displayScale) private var displayScale
    @Environment(\.onScreen) private var onScreen
    let computer: String
    let feed: ScreenFeed
    let typing: Bool
    @State private var inside = false
    @State private var tries = 0
    @State private var copies = 0

    /// The screen is watched only while it shows here and is shared, and at once when either
    /// comes back.
    private struct Watch: Equatable {
        let onScreen: Bool
        let shared: Bool
        let tries: Int
    }

    var body: some View {
        let showing = feed.showing
        let pictured = showing == .live || showing == .resting
        GeometryReader { area in
            let placement = feed.placement(in: area.size)
            ZStack {
                theme.colors.terminal
                VideoView(feed: feed, placement: placement, resting: showing == .resting)
                    .opacity(pictured ? 1 : 0)
                if pictured, feed.access?.input != false {
                    ScreenInputArea(feed: feed, typing: typing, inside: $inside)
                }
                if pictured {
                    ScreenPointer(feed: feed, placement: placement, size: area.size)
                        .opacity(inside ? 0 : 1)
                }
                VideoCover(showing: showing, computer: computer, failed: String(localized: "Can't show the screen", bundle: .module),
                           failure: feed.failure, identifier: "canvas.screen") { tries += 1 }
                CopiedNotice(copies: copies)
            }
        }
        .animation(theme.motion.fade, value: showing)
        .clipped()
        .onGeometryChange(for: CGSize.self) { $0.size } action: { feed.viewSize = $0 }
        .onChange(of: displayScale, initial: true) { _, scale in feed.viewScale = scale }
        // The input area goes without hearing that the pointer left it.
        .onChange(of: feed.access?.input) { _, input in
            if input == false {
                inside = false
            }
        }
        // The runtime keeps the pipe waiting while the computer is offline; one that broke meanwhile
        // opens again once the computer is back.
        .onChange(of: feed.offline) { _, offline in
            if !offline, feed.failure != nil {
                tries += 1
            }
        }
        .task(id: Watch(onScreen: onScreen, shared: feed.shared, tries: tries)) {
            if onScreen, feed.shared {
                feed.copied = { words in
                    Pasteboard.copy(words)
                    copies += 1
                }
                await feed.watch()
            }
        }
    }
}

/// The computer's pointer where the local one does not stand in for it, with its shape there,
/// growing and shrinking with the picture but kept large enough to find.
private struct ScreenPointer: View {
    private static let smallest: CGFloat = 0.6

    let feed: ScreenFeed
    let placement: VideoPlacement?
    let size: CGSize

    var body: some View {
        if let placement, let pointer = feed.pointer, let shape = feed.shapes[pointer.shape], shape.size.width > 0 {
            let scale = max(placement.fit(in: size), Self.smallest)
            let at = placement.viewPoint(CGPoint(x: pointer.x, y: pointer.y), in: size)
            Image(decorative: shape.image, scale: CGFloat(shape.image.width) / shape.size.width)
                .resizable()
                .frame(width: shape.size.width * scale, height: shape.size.height * scale)
                .position(x: at.x + (shape.size.width / 2 - shape.hotspot.x) * scale,
                          y: at.y + (shape.size.height / 2 - shape.hotspot.y) * scale)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
        }
    }
}

/// Why the picture takes no input, and where the person at the computer allows it, which a
/// watcher would not guess from a picture that looks live.
private struct UncontrolledLine: View {
    @Environment(\.theme) private var theme
    let computer: String

    var body: some View {
        Text("\(computer) does not allow control. Allow it in Kodosi's settings there.", bundle: .module)
            .appTextStyle(.footnote).fontWeight(.medium).foregroundStyle(theme.colors.caution)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12).padding(.bottom, 6)
            .accessibilityIdentifier("canvas.screen.uncontrolled")
    }
}

private struct ScreenBar: View {
    @Environment(\.theme) private var theme
    let feed: ScreenFeed
    @State private var picking = false

    var body: some View {
        HStack(spacing: 4) {
            sources
            if feed.zoom > 1 {
                IconButton(title: "Show the whole screen", bundle: .module, symbol: "arrow.down.right.and.arrow.up.left",
                           identifier: "canvas.screen.whole", size: control)
                {
                    feed.showWhole()
                }
                .transition(.opacity)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 6).frame(height: control + 8)
        .animation(theme.motion.snappy, value: feed.zoom > 1)
    }

    private var sourceFace: some View {
        HStack(spacing: 6) {
            Image(systemName: Self.symbol(of: feed.source)).font(scaled: 11, weight: .semibold).foregroundStyle(theme.colors.inkMuted)
            Text(name(of: feed.source)).lineLimit(1).truncationMode(.middle).foregroundStyle(theme.colors.ink)
            Image(systemName: "chevron.down").font(scaled: 8.5, weight: .bold).foregroundStyle(theme.colors.inkFaint)
        }
        .appTextStyle(.footnote).fontWeight(.medium)
        .padding(.horizontal, 10).frame(height: control - 2).frame(maxWidth: 280)
        .wellCapsule()
        .contentShape(Capsule())
    }

    private var sources: some View {
        #if os(macOS)
            Button { picking.toggle() } label: { sourceFace }
                .buttonStyle(PressScaleStyle())
                .help(String(localized: "Show another display or window", bundle: .module))
                .accessibilityLabel(Text(name(of: feed.source)))
                .accessibilityHint(Text("Show another display or window", bundle: .module))
                .accessibilityIdentifier("canvas.screen.source")
                .kodosiPopover(isPresented: $picking, arrowEdge: .bottom, alignment: .leading) {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 1) {
                            ForEach(choices, id: \.source) { choice in
                                Button {
                                    picking = false
                                    feed.show(choice.source)
                                } label: {
                                    MenuRow(title: choice.title, symbol: Self.symbol(of: choice.source), detail: choice.detail,
                                            selected: choice.source == feed.source)
                                }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier("canvas.screen.source.\(choice.identifier)")
                            }
                        }
                        .padding(6)
                    }
                    .frame(width: 320).frame(maxHeight: 380).fixedSize(horizontal: false, vertical: true)
                }
        #else
            Menu {
                ForEach(choices, id: \.source) { choice in
                    Button { feed.show(choice.source) } label: {
                        Label { Text(choice.detail.map { "\(choice.title) – \($0)" } ?? choice.title) } icon: {
                            Image(systemName: choice.source == feed.source ? "checkmark" : Self.symbol(of: choice.source))
                        }
                    }
                }
            } label: {
                sourceFace
            }
            .accessibilityLabel(Text(name(of: feed.source)))
            .accessibilityHint(Text("Show another display or window", bundle: .module))
            .accessibilityIdentifier("canvas.screen.source")
        #endif
    }

    private struct Choice {
        let source: ScreenSource
        let title: String
        let detail: String?
        let identifier: String
    }

    private var choices: [Choice] {
        let displays = feed.sources.displays.map { display in
            Choice(source: .display(display.id), title: displayName(display.id),
                   detail: "\(Int(display.width.rounded())) × \(Int(display.height.rounded()))", identifier: "display.\(display.id)")
        }
        let windows = feed.sources.windows.map { window in
            Choice(source: .window(window.id), title: window.title.isEmpty ? window.app : window.title,
                   detail: window.title.isEmpty ? nil : window.app, identifier: "window.\(window.id)")
        }
        return displays + windows
    }

    private func name(of source: ScreenSource?) -> String {
        switch source {
        case let .display(id):
            displayName(id)
        case let .window(id):
            feed.sources.windows.first { $0.id == id }.map { $0.title.isEmpty ? $0.app : $0.title }
                ?? String(localized: "Window", bundle: .module)
        case nil:
            String(localized: "Screen", bundle: .module)
        }
    }

    private func displayName(_ id: UInt32) -> String {
        let number = (feed.sources.displays.firstIndex { $0.id == id } ?? 0) + 1
        return number == 1 ? String(localized: "Main display", bundle: .module) : String(localized: "Display \(number)", bundle: .module)
    }

    private static func symbol(of source: ScreenSource?) -> String {
        if case .window = source {
            return "macwindow"
        }
        return "display"
    }
}
