import SwiftUI
#if os(macOS)
    import AppKit
#endif

public extension EnvironmentValues {
    @Entry var closePanel: () -> Void = {}
}

public extension View {
    func kodosiPopover(
        isPresented: Binding<Bool>, arrowEdge: Edge = .bottom, @ViewBuilder content: @escaping () -> some View
    ) -> some View {
        #if os(macOS)
            modifier(KodosiPopoverSource(isPresented: isPresented, edge: arrowEdge, popover: content))
        #else
            popover(isPresented: isPresented, arrowEdge: arrowEdge) {
                content().environment(\.closePanel) { isPresented.wrappedValue = false }
            }
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
        #else
            self
        #endif
    }
}

#if os(macOS)
    private struct KodosiPopoverSource<Popover: View>: ViewModifier {
        @Binding var isPresented: Bool
        let edge: Edge
        let popover: () -> Popover
        @State private var id = UUID()

        func body(content: Content) -> some View {
            content.anchorPreference(key: KodosiPopoverKey.self, value: .bounds) { anchor in
                guard isPresented else { return [] }
                let close = { isPresented = false }
                return [KodosiPopoverItem(
                    id: id, anchor: anchor, edge: edge, content: AnyView(popover().environment(\.closePanel, close)), dismiss: close
                )]
            }
        }
    }

    struct KodosiPopoverItem {
        let id: UUID
        let anchor: Anchor<CGRect>
        let edge: Edge
        let content: AnyView
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
            ZStack(alignment: .topLeading) {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture(perform: item.dismiss)
                    .accessibilityHidden(true)
                item.content
                    .fixedSize()
                    .clipShape(shape)
                    .background { RaisedBackground(shape: shape, fill: theme.colors.raised, elevation: .lifted) }
                    .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
                    .scaleEffect(shown || theme.motion.reduced ? 1 : 0.94, anchor: place.anchor)
                    .opacity(shown ? 1 : 0)
                    .offset(x: place.origin.x, y: place.origin.y)
            }
            .onAppear {
                KodosiPopoverState.add(1)
                withAnimation(theme.motion.snappy) { shown = true }
                monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                    guard event.keyCode == 53 else { return event }
                    item.dismiss()
                    return nil
                }
            }
            .onDisappear {
                KodosiPopoverState.add(-1)
                if let monitor {
                    NSEvent.removeMonitor(monitor)
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
            let origin = switch edge {
            case .bottom: CGPoint(x: anchor.midX - size.width / 2, y: anchor.maxY + gap)
            case .top: CGPoint(x: anchor.midX - size.width / 2, y: anchor.minY - gap - size.height)
            case .trailing: CGPoint(x: anchor.maxX + gap, y: anchor.midY - size.height / 2)
            case .leading: CGPoint(x: anchor.minX - gap - size.width, y: anchor.midY - size.height / 2)
            }
            let clamped = CGPoint(
                x: min(max(origin.x, Self.margin), max(Self.margin, bounds.width - size.width - Self.margin)),
                y: min(max(origin.y, Self.margin), max(Self.margin, bounds.height - size.height - Self.margin))
            )
            let unit = switch edge {
            case .bottom: UnitPoint.top
            case .top: UnitPoint.bottom
            case .trailing: UnitPoint.leading
            case .leading: UnitPoint.trailing
            }
            return (clamped, unit)
        }
    }
#endif
