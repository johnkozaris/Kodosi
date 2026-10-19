import SwiftUI
#if os(macOS)
    import AppKit
#endif

public extension View {
    func tip(_ text: LocalizedStringKey) -> some View {
        tip(Text(text))
    }

    func tip(verbatim text: String) -> some View {
        tip(Text(verbatim: text))
    }

    func tip(_ text: Text) -> some View {
        #if os(macOS)
            modifier(KodosiTipSource(text: text))
        #else
            help(text)
        #endif
    }
}

#if os(macOS)
    @MainActor
    enum KodosiTipClock {
        private static var open = 0
        private static var closed: ContinuousClock.Instant?

        static var warm: Bool {
            open > 0 || closed.map { $0.duration(to: .now) < .milliseconds(600) } ?? false
        }

        static func opened() {
            open += 1
        }

        static func closedOne() {
            open = max(0, open - 1)
            closed = .now
        }
    }

    private struct KodosiTipSource: ViewModifier {
        let text: Text
        @State private var shown = false
        @State private var instant = false
        @State private var muted = false
        @State private var waiting: Task<Void, Never>?
        @State private var id = UUID()

        func body(content: Content) -> some View {
            content
                .onHover { inside in
                    waiting?.cancel()
                    guard inside else {
                        shown = false
                        muted = false
                        return
                    }
                    guard !muted else { return }
                    if KodosiTipClock.warm {
                        instant = true
                        shown = true
                        return
                    }
                    waiting = Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(450))
                        guard !Task.isCancelled else { return }
                        instant = false
                        shown = true
                    }
                }
                .anchorPreference(key: KodosiTipKey.self, value: .bounds) { anchor in
                    guard shown else { return [] }
                    return [KodosiTipItem(id: id, anchor: anchor, text: text, instant: instant) {
                        shown = false
                        muted = true
                    }]
                }
                .onDisappear {
                    waiting?.cancel()
                    shown = false
                }
        }
    }

    struct KodosiTipItem {
        let id: UUID
        let anchor: Anchor<CGRect>
        let text: Text
        let instant: Bool
        let dismiss: () -> Void
    }

    struct KodosiTipKey: PreferenceKey {
        static var defaultValue: [KodosiTipItem] {
            []
        }

        static func reduce(value: inout [KodosiTipItem], nextValue: () -> [KodosiTipItem]) {
            value += nextValue()
        }
    }

    struct KodosiTipSurface: View {
        @Environment(\.theme) private var theme
        let item: KodosiTipItem
        let anchor: CGRect
        let bounds: CGSize
        @State private var size = CGSize.zero
        @State private var shown = false
        @State private var monitor: Any?

        private static let gap: CGFloat = 6
        private static let margin: CGFloat = 8

        var body: some View {
            let place = place
            item.text
                .appTextStyle(.caption)
                .foregroundStyle(theme.colors.ink)
                .lineLimit(1)
                .fixedSize()
                .padding(.horizontal, 8).padding(.vertical, 5)
                .background {
                    RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.sm, style: .continuous), fill: theme.colors.lifted,
                                     elevation: .lifted)
                }
                .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
                .scaleEffect(shown ? 1 : 0.96, anchor: place.anchor)
                .opacity(size == .zero ? 0 : shown ? 1 : 0)
                .offset(x: place.origin.x, y: place.origin.y)
                .onAppear {
                    KodosiTipClock.opened()
                    if item.instant || theme.motion.reduced {
                        shown = true
                    } else {
                        withAnimation(theme.motion.hover) { shown = true }
                    }
                    monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .keyDown]) { event in
                        item.dismiss()
                        return event
                    }
                }
                .onDisappear {
                    KodosiTipClock.closedOne()
                    if let monitor {
                        NSEvent.removeMonitor(monitor)
                    }
                }
        }

        private var place: (origin: CGPoint, anchor: UnitPoint) {
            let below = anchor.maxY + Self.gap + size.height <= bounds.height - Self.margin
            let y = below ? anchor.maxY + Self.gap : anchor.minY - Self.gap - size.height
            let x = min(max(anchor.midX - size.width / 2, Self.margin), max(Self.margin, bounds.width - size.width - Self.margin))
            let across = min(max((anchor.midX - x) / max(size.width, 1), 0), 1)
            return (CGPoint(x: x, y: y), UnitPoint(x: across, y: below ? 0 : 1))
        }
    }
#endif
