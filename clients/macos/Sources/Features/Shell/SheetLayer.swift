import AppKit
import KodosiDesign
import SwiftUI

extension EnvironmentValues {
    /// Closes the sheet that shows this view.
    @Entry var closeSheet: () -> Void = {}
}

/// A sheet of the app's own: the window dims, and the sheet rises in the
/// middle on the surface of the window's panels. A click beside it closes it;
/// Escape does what the sheet's own Cancel does, which is to close it unless
/// the sheet says otherwise.
struct SheetLayer<Content: View>: View {
    @Environment(\.theme) private var theme
    let close: () -> Void
    var cancel: (() -> Void)?
    @ViewBuilder let content: Content
    @State private var risen = false
    @State private var monitor: Any?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sheet, style: .continuous)
        GeometryReader { proxy in
            ZStack {
                theme.colors.shadow.opacity(theme.isDark ? 0.55 : 0.22)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: close)
                    .accessibilityHidden(true)
                SheetFit(margin: 24) {
                    content.environment(\.closeSheet, close)
                }
                .clipShape(shape)
                .background { RaisedBackground(shape: shape, fill: theme.colors.surface, elevation: .floating) }
                .scaleEffect(risen || theme.motion.reduced ? 1 : 0.95)
                .offset(y: risen || theme.motion.reduced ? 0 : 18)
                .opacity(risen ? 1 : 0)
                .accessibilityAddTraits(.isModal)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
        .ignoresSafeArea()
        .onAppear {
            withAnimation(theme.motion.spring) { risen = true }
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

/// Gives a sheet the size that it asks for, but no more than the window holds.
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
