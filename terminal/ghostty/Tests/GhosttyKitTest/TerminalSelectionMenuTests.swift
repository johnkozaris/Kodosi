import AppKit
import GhosttyKit
@testable import GhosttyTerminal
import Testing

@MainActor
@Suite(.serialized)
struct TerminalSelectionMenuTests {
    @Test
    func `context menu uses current native selection after scrolling and resizing`() throws {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController(configuration: .init { $0.withCustom("copy-on-select", "false") })
        let view = AppTerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        view.controller = controller
        view.configuration = .init(session: session)
        window.contentView = view
        defer {
            view.tearDownSurface()
            window.contentView = nil
        }
        let surface = try #require(view.surface)
        let raw = try #require(surface.rawValue)
        #expect(session.receive(Data((0 ..< 100).map { "line \($0) selection words\r\n" }.joined().utf8)))
        session.waitForPendingOutput()
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseDragged, .leftMouseUp] {
            let point = NSPoint(x: type == .leftMouseDown ? 10 : 70, y: view.bounds.height - 10)
            let event = try #require(NSEvent.mouseEvent(
                with: type, location: point, modifierFlags: [], timestamp: 1,
                windowNumber: window.windowNumber, context: nil, eventNumber: 1, clickCount: 1, pressure: 1
            ))
            switch type {
            case .leftMouseDown: view.mouseDown(with: event)
            case .leftMouseDragged: view.mouseDragged(with: event)
            default: view.mouseUp(with: event)
            }
        }
        #expect(surface.performBindingAction("select_all"))
        let selected = selection(raw)
        #expect(selected.contains("line 0"))
        #expect(surface.performBindingAction("scroll_page_up"))
        let size = try #require(surface.size())
        surface.setSize(width: size.widthPixels / 2, height: size.heightPixels)
        session.waitForPendingOutput()
        #expect(surface.performBindingAction("select_all"))
        let beforeClick = selection(raw)
        let point = NSPoint(x: view.bounds.midX, y: view.bounds.midY)
        let event = try #require(NSEvent.mouseEvent(
            with: .leftMouseDown, location: point, modifierFlags: [.control], timestamp: 1,
            windowNumber: window.windowNumber, context: nil, eventNumber: 1, clickCount: 1, pressure: 1
        ))
        let menu = try #require(view.menu(for: event))
        #expect(menu.items.count == 1)
        #expect(menu.items.first?.action == #selector(AppTerminalView.copy(_:)))
        #expect(selection(raw) == beforeClick)
        #expect(surface.performBindingAction("clear_screen"))
        #expect(session.receive(Data("\u{1B}[2J\u{1B}[3J\u{1B}[Hnew text".utf8)))
        session.waitForPendingOutput()
        #expect(surface.performBindingAction("select_all"))
        #expect(selection(raw).contains("new text"))
        #expect(view.selectionContextMenu() != nil)
    }

    @Test
    func `captured control click remains terminal input rather than a menu`() throws {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let view = AppTerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        view.controller = controller
        view.configuration = .init(session: session)
        window.contentView = view
        defer {
            view.tearDownSurface()
            window.contentView = nil
        }
        #expect(session.receive(Data("\u{1B}[?1000h\u{1B}[?1006h".utf8)))
        session.waitForPendingOutput()
        #expect(view.surface?.isMouseCaptured == true)
        let event = try #require(NSEvent.mouseEvent(
            with: .leftMouseDown, location: NSPoint(x: 20, y: 20), modifierFlags: [.control], timestamp: 1,
            windowNumber: window.windowNumber, context: nil, eventNumber: 1, clickCount: 1, pressure: 1
        ))
        #expect(view.menu(for: event) == nil)
    }

    private func selection(_ surface: ghostty_surface_t) -> String {
        var text = ghostty_text_s()
        guard ghostty_surface_read_selection(surface, &text), let pointer = text.text else { return "" }
        defer { ghostty_surface_free_text(surface, &text) }
        return String(decoding: UnsafeRawBufferPointer(start: pointer, count: Int(text.text_len)), as: UTF8.self)
    }
}
