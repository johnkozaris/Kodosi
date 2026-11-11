import AppKit
import SwiftUI

extension View {
    /// A double-click on empty space in the band of the title bar does what System Settings ›
    /// Desktop & Dock gives a double-click on a title bar: to fill the screen, unless the person
    /// chose to minimize or to do nothing.
    func titleBarGap() -> some View {
        modifier(TitleBarGap())
    }
}

private struct TitleBarGap: ViewModifier {
    @State private var id = UUID()

    func body(content: Content) -> some View {
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { TitleBarGaps.set(id, $0) }
            .onDisappear { TitleBarGaps.set(id, nil) }
    }
}

@MainActor
private enum TitleBarGaps {
    private static var gaps: [UUID: CGRect] = [:]
    private static var watching: Any?

    static func set(_ id: UUID, _ frame: CGRect?) {
        gaps[id] = frame
        guard watching == nil else { return }
        watching = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { event in
            MainActor.assumeIsolated { doubleClick(event) }
            return event
        }
    }

    /// The window's own title bar and its buttons answer a click that reaches them, so a gap takes
    /// only a double-click that lands on the content.
    private static func doubleClick(_ event: NSEvent) {
        guard event.clickCount == 2, let window = event.window, window === MainWindowLocator.mainWindow(),
              let content = window.contentView, let hit = content.superview?.hitTest(event.locationInWindow),
              hit.isDescendant(of: content)
        else { return }
        let local = content.convert(event.locationInWindow, from: nil)
        let point = CGPoint(x: local.x, y: content.isFlipped ? local.y : content.bounds.height - local.y)
        guard gaps.values.contains(where: { $0.minY < Metrics.titleBarHeight && $0.contains(point) }) else { return }
        switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
        case "Minimize": window.performMiniaturize(nil)
        case "None": break
        default: window.performZoom(nil)
        }
    }
}
