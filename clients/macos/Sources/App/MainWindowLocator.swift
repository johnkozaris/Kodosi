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
        titledWindow() ?? NSApp.windows.first(where: { $0.isVisible })
    }

    static func titledWindow() -> NSWindow? {
        NSApp.windows.filter { $0.styleMask.contains(.titled) }.max {
            $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height
        }
    }
}
