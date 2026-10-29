import KodosiBridge
import SwiftUI

/// The keys of a terminal's canvas, one for each of its parts: its files and its browser. The
/// chosen part shows its name. When the agent shows or changes a file, the files key shows that
/// file's name for a moment and its face flashes. With `flight`, the keys move between the
/// terminal's header and the canvas's header. On the canvas, a pointer on a key shows its name
/// and the key that closes the part, and a plus key lists the parts to open.
public struct CanvasDock: View {
    @Environment(\.theme) private var theme
    @Namespace private var thumb
    let session: RuntimeSession
    let store: CanvasStore
    let chosen: CanvasSection?
    let flight: Namespace.ID?
    let identifier: String
    let press: (CanvasSection) -> Void
    let close: ((CanvasSection) -> Void)?
    let open: ((CanvasSection) -> Void)?
    @State private var told: Told?
    @State private var tellings = 0
    @State private var pointed: CanvasSection?
    @State private var adding = false

    public init(session: RuntimeSession, store: CanvasStore, chosen: CanvasSection?, flight: Namespace.ID? = nil, identifier: String,
                press: @escaping (CanvasSection) -> Void, close: ((CanvasSection) -> Void)? = nil,
                open: ((CanvasSection) -> Void)? = nil)
    {
        self.session = session
        self.store = store
        self.chosen = chosen
        self.flight = flight
        self.identifier = identifier
        self.press = press
        self.close = close
        self.open = open
    }

    private struct Told: Equatable {
        let section: CanvasSection
        let words: String
    }

    private struct Change: Equatable {
        let key: String
        let changed: UInt64
    }

    public var body: some View {
        let canvas = session.canvas
        let sections = close == nil ? canvas?.sections ?? [] : store.sections(session)
        let closed = open == nil ? [] : store.closedSections(session)
        HStack(spacing: 4) {
            HStack(spacing: 2) {
                ForEach(sections) { section in
                    key(section, canvas: canvas)
                        .transition(.scale(scale: 0.7).combined(with: .opacity))
                }
            }
            .padding(2)
            .wellCapsule()
            .fixedSize()
            .modifier(Flight(id: session.id, space: flight))
            if let open, !closed.isEmpty {
                adder(closed, open: open)
                    .transition(.opacity)
            }
        }
        .animation(theme.motion.snappy, value: sections)
        .animation(theme.motion.snappy, value: told)
        .animation(theme.motion.snappy, value: chosen)
        .animation(theme.motion.snappy, value: pointed)
        .onChange(of: canvas?.items.max { $0.changed < $1.changed }.map { Change(key: $0.key, changed: $0.changed) }) { _, change in
            if let change, let item = canvas?.items.first(where: { $0.key == change.key }) {
                tell(Told(section: .files, words: item.name))
            }
        }
        .onAppear {
            if let newest = canvas?.items.max(by: { $0.changed < $1.changed }), store.isNew(.item(newest), in: session.id) {
                tell(Told(section: .files, words: newest.name))
            }
        }
        .onChange(of: canvas?.browser != nil) { _, browsing in
            if browsing {
                tell(Told(section: .browser, words: CanvasSection.browser.title))
            }
        }
        .task(id: tellings) {
            guard tellings > 0 else { return }
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled else { return }
            told = nil
        }
    }

    private func tell(_ news: Told) {
        told = news
        tellings += 1
    }

    private func key(_ section: CanvasSection, canvas: KodosiBridge.Canvas?) -> some View {
        let selected = section == chosen
        let closing = close != nil && pointed == section
        let words: String? = selected || closing ? section.title : told?.section == section ? told?.words : nil
        let count = section == .files ? canvas?.items.count ?? 0 : 0
        let news = !selected && (canvas?.shown.contains { $0.section == section && store.isNew($0, in: session.id) } ?? false)
        let newest = section == .files ? canvas?.items.map(\.changed).max() : nil
        let doing = section == .browser && canvas?.browser?.doing != nil
        return Button { press(section) } label: {
            HStack(spacing: 5) {
                Image(systemName: section.symbol)
                    .font(.system(size: 11.5, weight: .semibold))
                    .foregroundStyle(selected ? theme.colors.accentStrong : theme.colors.inkMuted)
                if let words {
                    Text(words).lineLimit(1).truncationMode(.middle)
                        .frame(maxWidth: 160, alignment: .leading)
                        .fixedSize(horizontal: true, vertical: false)
                        .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
                        .transition(.opacity.combined(with: .offset(x: -6)))
                } else if count > 1 {
                    Text(count, format: .number).monospacedDigit().contentTransition(.numericText())
                        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                }
            }
            .appTextStyle(.footnote).fontWeight(.medium)
            .padding(.leading, words == nil ? 7 : 9).padding(.trailing, (words == nil ? 7 : 11) + (closing ? 16 : 0))
            .frame(minWidth: 26).frame(height: 24)
            .background {
                if selected {
                    RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                        .matchedGeometryEffect(id: "thumb", in: thumb)
                }
            }
            .wash(trigger: newest, radius: 12)
            .overlay {
                if doing {
                    LightRim(.working, cornerRadius: 12, lineWidth: 1.5).transition(.opacity)
                }
            }
            .overlay(alignment: .topTrailing) {
                if news {
                    BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -1, y: 1)
                        .allowsHitTesting(false).transition(AnyTransition.pop)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help(section.title)
        .accessibilityLabel(Text(section.title))
        .accessibilityValue(count > 0 ? Text(count, format: .number) : Text(verbatim: ""))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("\(identifier).\(section.rawValue)")
        .accessibilityActions {
            if let close {
                Button { close(section) } label: { Text("Close", bundle: .module) }
            }
        }
        .modifier(CloseMenu(close: close.map { close in { close(section) } }))
        .overlay(alignment: .trailing) {
            if closing, let close {
                RemoveKey(size: 14, title: String(localized: "Close", bundle: .module),
                          identifier: "\(identifier).close.\(section.rawValue)") { close(section) }
                    .padding(.trailing, 5)
                    .transition(.opacity)
            }
        }
        .onHover { inside in
            if inside {
                pointed = section
            } else if pointed == section {
                pointed = nil
            }
        }
    }

    @ViewBuilder
    private func adder(_ closed: [CanvasSection], open: @escaping (CanvasSection) -> Void) -> some View {
        let plus = Image(systemName: "plus").font(.system(size: 10.5, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
            .frame(width: 24, height: 24).contentShape(Circle())
        #if os(macOS)
            Button { adding.toggle() } label: { plus }
                .buttonStyle(PressScaleStyle(scale: 0.88))
                .help(String(localized: "Open", bundle: .module))
                .accessibilityLabel(Text("Open", bundle: .module))
                .accessibilityIdentifier("\(identifier).open")
                .kodosiPopover(isPresented: $adding, arrowEdge: .bottom, alignment: .leading) {
                    VStack(alignment: .leading, spacing: 1) {
                        ForEach(closed) { section in
                            Button {
                                adding = false
                                open(section)
                            } label: {
                                MenuRow(title: section.title, symbol: section.symbol)
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("\(identifier).open.\(section.rawValue)")
                        }
                    }
                    .padding(6).frame(width: 184)
                }
        #else
            Menu {
                ForEach(closed) { section in
                    Button(section.title, systemImage: section.symbol) { open(section) }
                }
            } label: { plus }
                .accessibilityLabel(Text("Open", bundle: .module))
                .accessibilityIdentifier("\(identifier).open")
        #endif
    }
}

/// Closes a part from the menu of its key too, for a touch screen that has no pointer.
private struct CloseMenu: ViewModifier {
    let close: (() -> Void)?

    func body(content: Content) -> some View {
        if let close {
            content.contextMenu {
                Button(action: close) {
                    Label { Text("Close", bundle: .module) } icon: { Image(systemName: "xmark") }
                }
            }
        } else {
            content
        }
    }
}

/// Moves the keys between the terminal's header and the canvas's header.
private struct Flight: ViewModifier {
    let id: String
    let space: Namespace.ID?

    func body(content: Content) -> some View {
        if let space {
            content.matchedGeometryEffect(id: id, in: space)
        } else {
            content
        }
    }
}
