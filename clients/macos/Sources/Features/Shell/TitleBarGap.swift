import AppKit
import SwiftUI

extension View {
    /// Marks empty space in the custom title bar band so double-clicking it honors the system
    /// title-bar double-click action (zoom, minimize, or none).
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

    /// Only handle double-clicks hitting `contentView`; native title-bar chrome handles its own.
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
