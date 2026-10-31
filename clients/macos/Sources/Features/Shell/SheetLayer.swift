import AppKit
import KodosiDesign
import SwiftUI

struct SheetLayer<Content: View>: View {
    @Environment(\.theme) private var theme
    let close: () -> Void
    var cancel: (() -> Void)?
    var rises = true
    @ViewBuilder let content: Content
    @State private var risen = false
    @State private var monitor: Any?
    @State private var panel = UUID()

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sheet, style: .continuous)
        let risen = risen || !rises
        GeometryReader { proxy in
            ZStack {
                theme.colors.shadow.opacity(theme.isDark ? 0.55 : 0.22)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: close)
                    .accessibilityHidden(true)
                SheetFit(margin: 24) {
                    content.environment(\.closePanel, ClosePanelAction(panel: panel, close: close))
                }
                .clipShape(shape)
                .background { RaisedBackground(shape: shape, fill: theme.colors.surface, elevation: .floating) }
                .scaleEffect(risen || theme.motion.reduced ? 1 : 0.95)
                .offset(y: risen || theme.motion.reduced ? 0 : 18)
                .opacity(risen ? 1 : 0)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
        .ignoresSafeArea()
        .onAppear {
            withAnimation(theme.motion.spring) { self.risen = true }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53, !KodosiPopoverState.shown else { return event }
                (cancel ?? close)()
                return nil
            }
        }
        .onDisappear {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
        }
    }
}

private struct SheetFit: Layout {
    let margin: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        guard let sheet = subviews.first else { return .zero }
        return sheet.sizeThatFits(fit(sheet, in: proposal))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        guard let sheet = subviews.first else { return }
        sheet.place(at: CGPoint(x: bounds.midX, y: bounds.midY), anchor: .center, proposal: fit(sheet, in: proposal))
    }

    private func fit(_ sheet: LayoutSubview, in proposal: ProposedViewSize) -> ProposedViewSize {
        let ideal = sheet.sizeThatFits(.unspecified)
        let room = proposal.replacingUnspecifiedDimensions(by: ideal)
        return ProposedViewSize(width: min(ideal.width, room.width - margin * 2), height: min(ideal.height, room.height - margin * 2))
    }
}
