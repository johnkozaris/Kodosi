import AppKit

@MainActor
enum MainWindowLocator {
    static func activate() {
        NSApp.activate(ignoringOtherApps: true)
        if let window = mainWindow() {
            window.deminiaturize(nil)
            window.makeKeyAndOrderFront(nil)
        }
    }

    static func mainWindow() -> NSWindow? {
        let candidates = NSApp.windows.filter {
            $0.styleMask.contains(.titled)
        }
        return candidates.max {
            $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height
        } ?? NSApp.windows.first(where: { $0.isVisible })
    }
}
