import SwiftUI
#if os(macOS)
    import AppKit
#endif

public struct ClosePanelAction: Equatable {
    private var panel: UUID?
    private var close: () -> Void = {}

    public init() {}

    public init(panel: UUID, close: @escaping () -> Void) {
        self.panel = panel
        self.close = close
    }

    public func callAsFunction() {
        close()
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.panel == rhs.panel
    }
}

public extension EnvironmentValues {
    @Entry var closePanel = ClosePanelAction()
}

public extension View {
    func kodosiPopover(
        isPresented: Binding<Bool>, arrowEdge: Edge = .bottom, alignment: HorizontalAlignment = .center,
        @ViewBuilder content: @escaping () -> some View,
    ) -> some View {
        #if os(macOS)
            modifier(KodosiPopoverSource(isPresented: isPresented, edge: arrowEdge, alignment: alignment, popover: content))
        #else
            modifier(KodosiPopoverSource(isPresented: isPresented, edge: arrowEdge, popover: content))
        #endif
    }

    func kodosiPopoverLayer() -> some View {
        #if os(macOS)
            overlayPreferenceValue(KodosiPopoverKey.self) { items in
                GeometryReader { proxy in
                    if let item = items.last {
                        KodosiPopoverSurface(item: item, anchor: proxy[item.anchor], bounds: proxy.size).id(item.id)
                    }
                }
            }
            .overlayPreferenceValue(KodosiTipKey.self) { items in
                GeometryReader { proxy in
                    if let item = items.last {
                        KodosiTipSurface(item: item, anchor: proxy[item.anchor], bounds: proxy.size).id(item.id)
                    }
                }
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            }
        #else
            self
        #endif
    }
}

#if !os(macOS)
    private struct KodosiPopoverSource<Popover: View>: ViewModifier {
        @Binding var isPresented: Bool
        let edge: Edge
        let popover: () -> Popover
        @State private var id = UUID()

        func body(content: Content) -> some View {
            content.popover(isPresented: $isPresented, arrowEdge: edge) {
                popover().environment(\.closePanel, ClosePanelAction(panel: id) { isPresented = false })
            }
        }
    }
#endif

#if os(macOS)
    public extension View {
        /// Shows `content` beside the popover that holds this view, level with this view.
        func kodosiFlyout(isPresented: Bool, @ViewBuilder content: @escaping () -> some View) -> some View {
            anchorPreference(key: KodosiFlyoutKey.self, value: .bounds) { anchor in
                isPresented ? [KodosiFlyoutItem(anchor: anchor, content: AnyView(content()))] : []
            }
        }
    }

    struct KodosiFlyoutItem {
        let anchor: Anchor<CGRect>
        let content: AnyView
    }

    struct KodosiFlyoutKey: PreferenceKey {
        static var defaultValue: [KodosiFlyoutItem] {
            []
        }

        static func reduce(value: inout [KodosiFlyoutItem], nextValue: () -> [KodosiFlyoutItem]) {
            value += nextValue()
        }
    }

    private struct KodosiPopoverSource<Popover: View>: ViewModifier {
        @Environment(\.theme) private var theme
        @Binding var isPresented: Bool
        let edge: Edge
        let alignment: HorizontalAlignment
        let popover: () -> Popover
        @State private var id = UUID()
        @State private var leaving = false

        func body(content: Content) -> some View {
            content
                .anchorPreference(key: KodosiPopoverKey.self, value: .bounds) { anchor in
                    guard isPresented || leaving else { return [] }
                    let close = { isPresented = false }
                    return [KodosiPopoverItem(
                        id: id, anchor: anchor, edge: edge, alignment: alignment,
                        content: AnyView(popover().environment(\.closePanel, ClosePanelAction(panel: id, close: close))),
                        closing: !isPresented, dismiss: close,
                    )]
                }
                .onChange(of: isPresented) { _, now in
                    guard !now, !theme.motion.reduced else { return }
                    leaving = true
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(140))
                        if !isPresented {
                            leaving = false
                        }
                    }
                }
        }
    }

    struct KodosiPopoverItem {
        let id: UUID
        let anchor: Anchor<CGRect>
        let edge: Edge
        let alignment: HorizontalAlignment
        let content: AnyView
        let closing: Bool
        let dismiss: () -> Void
    }

    struct KodosiPopoverKey: PreferenceKey {
        static var defaultValue: [KodosiPopoverItem] {
            []
        }

        static func reduce(value: inout [KodosiPopoverItem], nextValue: () -> [KodosiPopoverItem]) {
            value += nextValue()
        }
    }

    @MainActor
    public enum KodosiPopoverState {
        private static var count = 0

        public static var shown: Bool {
            count > 0
        }

        fileprivate static func add(_ change: Int) {
            count = max(0, count + change)
        }
    }

    private struct KodosiPopoverSurface: View {
        @Environment(\.theme) private var theme
        let item: KodosiPopoverItem
        let anchor: CGRect
        let bounds: CGSize
        @State private var size = CGSize.zero
        @State private var shown = false
        @State private var monitor: Any?

        private static let gap: CGFloat = 6
        private static let margin: CGFloat = 10

        var body: some View {
            let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
            let place = place
            let visible = shown && !item.closing
            ZStack(alignment: .topLeading) {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture(perform: item.dismiss)
                    .accessibilityHidden(true)
                item.content
                    .fixedSize()
                    .clipShape(shape)
                    .background { RaisedBackground(shape: shape, fill: theme.colors.raised, elevation: .lifted) }
                    .overlayPreferenceValue(KodosiFlyoutKey.self) { flyouts in
                        GeometryReader { proxy in
                            if let flyout = flyouts.last {
                                KodosiFlyoutSurface(
                                    content: flyout.content, row: proxy[flyout.anchor],
                                    popover: CGRect(origin: place.origin, size: size), bounds: bounds,
                                )
                            }
                        }
                    }
                    .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
                    .scaleEffect(visible || theme.motion.reduced ? 1 : item.closing ? 0.97 : 0.94, anchor: place.anchor)
                    .opacity(visible ? 1 : 0)
                    .offset(x: place.origin.x, y: place.origin.y)
            }
            .allowsHitTesting(!item.closing)
            .animation(theme.motion.hover, value: item.closing)
            .onAppear {
                withAnimation(theme.motion.snappy) { shown = true }
                acquire()
            }
            .onChange(of: item.closing) { _, closing in
                if closing {
                    release()
                } else {
                    acquire()
                }
            }
            .onDisappear(perform: release)
        }

        private func acquire() {
            guard monitor == nil else { return }
            KodosiPopoverState.add(1)
            let dismiss = item.dismiss
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53 else { return event }
                dismiss()
                return nil
            }
        }

        private func release() {
            guard let monitor else { return }
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
            KodosiPopoverState.add(-1)
        }

        private struct KodosiFlyoutSurface: View {
            @Environment(\.theme) private var theme
            let content: AnyView
            let row: CGRect
            let popover: CGRect
            let bounds: CGSize
            @State private var size = CGSize.zero
            @State private var shown = false

            var body: some View {
                let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                let (gap, margin) = (KodosiPopoverSurface.gap, KodosiPopoverSurface.margin)
                let trailing = popover.maxX + gap + size.width <= bounds.width - margin
                let x = trailing ? popover.width + gap : -gap - size.width
                let y = max(min(row.minY - 6, bounds.height - margin - popover.minY - size.height), margin - popover.minY)
                content
                    .fixedSize()
                    .clipShape(shape)
                    .background { RaisedBackground(shape: shape, fill: theme.colors.raised, elevation: .lifted) }
                    .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
                    .opacity(shown ? 1 : 0)
                    .offset(x: x + (shown || theme.motion.reduced ? 0 : trailing ? -8 : 8), y: y)
                    .onAppear {
                        withAnimation(theme.motion.snappy) { shown = true }
                    }
            }
        }

        private var place: (origin: CGPoint, anchor: UnitPoint) {
            let gap = Self.gap
            var edge = item.edge
            switch edge {
            case .bottom where anchor.maxY + gap + size.height > bounds.height - Self.margin && anchor.minY - gap - size.height > 0:
                edge = .top
            case .top where anchor.minY - gap - size.height < Self.margin:
                edge = .bottom
            case .trailing where anchor.maxX + gap + size.width > bounds.width - Self.margin:
                edge = .leading
            case .leading where anchor.minX - gap - size.width < Self.margin:
                edge = .trailing
            default:
                break
            }
            let x = item.alignment == .leading ? anchor.minX
                : item.alignment == .trailing ? anchor.maxX - size.width : anchor.midX - size.width / 2
            let origin = switch edge {
            case .bottom: CGPoint(x: x, y: anchor.maxY + gap)
            case .top: CGPoint(x: x, y: anchor.minY - gap - size.height)
            case .trailing: CGPoint(x: anchor.maxX + gap, y: anchor.midY - size.height / 2)
            case .leading: CGPoint(x: anchor.minX - gap - size.width, y: anchor.midY - size.height / 2)
            }
            let clamped = CGPoint(
                x: min(max(origin.x, Self.margin), max(Self.margin, bounds.width - size.width - Self.margin)),
                y: min(max(origin.y, Self.margin), max(Self.margin, bounds.height - size.height - Self.margin)),
            )
            let across = min(max((anchor.midX - clamped.x) / max(size.width, 1), 0), 1)
            let down = min(max((anchor.midY - clamped.y) / max(size.height, 1), 0), 1)
            let unit = switch edge {
            case .bottom: UnitPoint(x: across, y: 0)
            case .top: UnitPoint(x: across, y: 1)
            case .trailing: UnitPoint(x: 0, y: down)
            case .leading: UnitPoint(x: 1, y: down)
            }
            return (clamped, unit)
        }
    }
#endif
