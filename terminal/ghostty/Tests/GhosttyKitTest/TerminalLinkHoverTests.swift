import AppKit
import GhosttyKit
@testable import GhosttyTerminal
import Testing

@MainActor
@Suite(.serialized)
struct TerminalLinkHoverTests {
    @Test
    func `a tap reads the link under it at once, for a program link and an address in the text`() throws {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let view = AppTerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        view.controller = TerminalController()
        view.configuration = .init(session: session)
        window.contentView = view
        defer {
            view.tearDownSurface()
            window.contentView = nil
        }
        let surface = try #require(view.surface)
        let output = "\u{1B}]8;;https://example.com/pull/1\u{1B}\\open the pull request\u{1B}]8;;\u{1B}\\\r\n"
            + "see https://example.com/docs now\r\n"
        #expect(session.receive(Data(output.utf8)))
        session.waitForPendingOutput()
        let size = try #require(surface.size())
        let cellWidth = Double(size.cellWidthPixels) / view.core.scaleFactor()
        let cellHeight = Double(size.cellHeightPixels) / view.core.scaleFactor()
        let none = ghostty_input_mods_e(rawValue: 0)

        // Mirrors iOS tap probing: move off-surface, hover with Command, then move off-surface.
        func link(column: Double, row: Double) -> String? {
            surface.sendMousePos(x: -1, y: -1, mods: none)
            view.core.bridge.hoverLink = nil
            surface.sendMousePos(
                x: (column + 0.5) * cellWidth,
                y: (row + 0.5) * cellHeight,
                mods: TerminalInputModifiers.super_.ghosttyMods
            )
            let link = view.core.bridge.hoverLink
            surface.sendMousePos(x: -1, y: -1, mods: none)
            #expect(view.core.bridge.hoverLink == nil)
            return link
        }

        #expect(link(column: 3, row: 0) == "https://example.com/pull/1")
        #expect(link(column: 3, row: 0) == "https://example.com/pull/1")
        #expect(link(column: 10, row: 1) == "https://example.com/docs")
        #expect(link(column: 1, row: 1) == nil)
        surface.sendMousePos(x: 3.5 * cellWidth, y: 0.5 * cellHeight, mods: none)
        #expect(view.core.bridge.hoverLink == nil)
    }

    @Test
    func `the pointer shows text, a link and the shape that the program asks for`() async throws {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let view = AppTerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        view.controller = TerminalController()
        view.configuration = .init(session: session)
        window.contentView = view
        defer {
            view.tearDownSurface()
            window.contentView = nil
        }
        let surface = try #require(view.surface)
        #expect(view.pointer == .iBeam)

        let output = "\u{1B}]22;crosshair\u{1B}\\\u{1B}]8;;https://example.com/pull/1\u{1B}\\open the pull request\u{1B}]8;;\u{1B}\\"
        #expect(session.receive(Data(output.utf8)))
        session.waitForPendingOutput()
        #expect(try await pointer(of: view, becomes: .crosshair))

        let size = try #require(surface.size())
        let cellWidth = Double(size.cellWidthPixels) / view.core.scaleFactor()
        let cellHeight = Double(size.cellHeightPixels) / view.core.scaleFactor()
        surface.sendMousePos(x: 3.5 * cellWidth, y: 0.5 * cellHeight, mods: TerminalInputModifiers.super_.ghosttyMods)
        #expect(try await pointer(of: view, becomes: .pointingHand))
        surface.sendMousePos(x: -1, y: -1, mods: ghostty_input_mods_e(rawValue: 0))
        #expect(try await pointer(of: view, becomes: .crosshair))

        #expect(session.receive(Data("\u{1B}]22;\u{1B}\\".utf8)))
        session.waitForPendingOutput()
        #expect(try await pointer(of: view, becomes: .iBeam))
    }

    @Test
    func `a restored view shows the arrow over a program that captures the mouse`() async throws {
        let captured = try checkpoint("mouse-capture-checkpoint")
        let released = try checkpoint("mouse-release-checkpoint")
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let view = AppTerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        view.controller = TerminalController()
        view.configuration = .init(session: session)
        window.contentView = view
        defer {
            view.tearDownSurface()
            window.contentView = nil
        }
        _ = try #require(view.surface)

        #expect(session.restoreCheckpointSynchronously(captured, console: false))
        #expect(try await pointer(of: view, becomes: .arrow))
        #expect(session.receive(Data("\u{1B}[?1003l".utf8)))
        session.waitForPendingOutput()
        #expect(try await pointer(of: view, becomes: .iBeam))

        #expect(session.restoreCheckpointSynchronously(captured, console: false))
        #expect(try await pointer(of: view, becomes: .arrow))
        #expect(session.restoreCheckpointSynchronously(released, console: false))
        #expect(try await pointer(of: view, becomes: .iBeam))
    }

    private func checkpoint(_ name: String) throws -> Data {
        try Data(contentsOf: #require(Bundle.module.url(forResource: name, withExtension: "snapshot")))
    }

    private func pointer(of view: AppTerminalView, becomes cursor: NSCursor) async throws -> Bool {
        for _ in 0 ..< 100 where view.pointer != cursor {
            try await Task.sleep(for: .milliseconds(20))
        }
        return view.pointer == cursor
    }
}
