#if os(macOS)
    import AppKit
    import GhosttyTerminal
    @testable import KodosiTerminal
    import SwiftUI
    import Synchronization
    import Testing

    private final class LockedByteRecorder: Sendable {
        private let storage = Mutex<[Data]>([])

        var values: [Data] {
            storage.withLock { $0 }
        }

        func append(_ value: Data) {
            storage.withLock { $0.append(value) }
        }
    }

    @MainActor
    private final class AccessibilityUpdateRecorder {
        private(set) var count = 0

        func record() {
            count += 1
        }
    }

    @MainActor
    @Test func `accessibility refresh leaves the main actor free while output continues`() async {
        let relay = TerminalAccessibilityUpdateRelay()
        let refreshes = AccessibilityUpdateRecorder()
        let token = relay.register {
            refreshes.record()
            let busy = ContinuousClock.now + .milliseconds(2)
            while ContinuousClock.now < busy {}
            return true
        }
        let output = Task.detached {
            let end = ContinuousClock.now + .milliseconds(500)
            while ContinuousClock.now < end {
                relay.signal()
                try? await Task.sleep(for: .milliseconds(1))
            }
        }
        var beats = 0
        let end = ContinuousClock.now + .milliseconds(500)
        while ContinuousClock.now < end {
            beats += 1
            try? await Task.sleep(for: .milliseconds(5))
        }
        await output.value
        relay.unregister(token)

        #expect(beats >= 20)
        #expect(refreshes.count <= 15)
    }

    @MainActor
    @Test func `renderer session deallocates after owner release`() {
        weak var weakSession: TerminalRendererSession?
        do {
            let session = TerminalRendererSession(
                write: { _ in },
                resize: { _, _ in },
            )
            weakSession = session
        }

        #expect(weakSession == nil)
    }

    @MainActor
    @Test func `read only renderer suppresses backend writes`() {
        let writes = LockedByteRecorder()
        let session = TerminalRendererSession(
            write: { writes.append($0) },
            resize: { _, _ in },
        )
        session.setAllowsInput(false)

        session.viewState.configuration.session.sendInput(Data("blocked".utf8))
        #expect(writes.values.isEmpty)

        session.setAllowsInput(true)
        session.viewState.configuration.session.sendInput(Data("allowed".utf8))
        #expect(writes.values == [Data("allowed".utf8)])
    }

    private func commandKey(_ character: String, keyCode: UInt16, timestamp: TimeInterval) throws -> NSEvent {
        try #require(NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: .command,
            timestamp: timestamp,
            windowNumber: 0,
            context: nil,
            characters: character,
            charactersIgnoringModifiers: character,
            isARepeat: false,
            keyCode: keyCode,
        ))
    }

    @MainActor
    @Test func `read only hosted view consumes input producing events`() async throws {
        let writes = LockedByteRecorder()
        let session = TerminalRendererSession(
            write: { writes.append($0) },
            resize: { _, _ in },
        )
        session.setAllowsInput(false)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 240),
            styleMask: [.titled],
            backing: .buffered,
            defer: false,
        )
        let view = TerminalRendererHostedView(frame: window.contentView?.bounds ?? .zero)
        view.allowsInput = false
        view.delegateProxy.target = session.viewState
        view.delegate = view.delegateProxy
        view.controller = session.viewState.controller
        view.configuration = session.viewState.configuration
        window.contentView = view
        window.makeKeyAndOrderFront(nil)
        defer {
            view.tearDownSurface()
            window.orderOut(nil)
        }
        for _ in 0 ..< 100 where session.nativeSurfaceGeneration == 0 {
            try await Task.sleep(for: .milliseconds(2))
        }
        #expect(session.nativeSurfaceGeneration != 0)

        let key = try commandKey("v", keyCode: 9, timestamp: 1)
        let mouse = try #require(NSEvent.mouseEvent(
            with: .mouseMoved,
            location: .zero,
            modifierFlags: [],
            timestamp: 1,
            windowNumber: 0,
            context: nil,
            eventNumber: 1,
            clickCount: 0,
            pressure: 0,
        ))

        #expect(view.performKeyEquivalent(with: key))
        let appShortcut = try commandKey("b", keyCode: 11, timestamp: 2)
        #expect(!view.performKeyEquivalent(with: appShortcut))
        view.keyDown(with: key)
        view.keyUp(with: key)
        view.flagsChanged(with: key)
        view.insertText("blocked", replacementRange: NSRange(location: NSNotFound, length: 0))
        view.mouseMoved(with: mouse)
        view.rightMouseDragged(with: mouse)
        view.otherMouseDragged(with: mouse)
        #expect(view.tryToPerform(NSSelectorFromString("paste:"), with: nil))
        #expect(session.receive(Data("read-only selection".utf8)))
        for _ in 0 ..< 100 where session.readViewportText()?.contains("read-only selection") != true {
            try await Task.sleep(for: .milliseconds(2))
        }
        view.selectAll(nil)
        #expect(view.menu(for: mouse)?.items.first?.action == NSSelectorFromString("copy:"))
        try await Task.sleep(for: .milliseconds(20))
        #expect(writes.values.isEmpty)
    }

    @MainActor
    @Test func `read only hosted view scrolls viewport without backend writes`() async throws {
        let writes = LockedByteRecorder()
        let session = TerminalRendererSession(
            write: { writes.append($0) },
            resize: { _, _ in },
        )
        session.setAllowsInput(false)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 160),
            styleMask: [.titled],
            backing: .buffered,
            defer: false,
        )
        let view = TerminalRendererHostedView(frame: window.contentView?.bounds ?? .zero)
        view.allowsInput = false
        view.delegateProxy.target = session.viewState
        view.delegate = view.delegateProxy
        view.controller = session.viewState.controller
        view.configuration = session.viewState.configuration
        window.contentView = view
        window.makeKeyAndOrderFront(nil)
        defer {
            view.tearDownSurface()
            window.orderOut(nil)
        }

        for _ in 0 ..< 100 where session.nativeSurfaceGeneration == 0 {
            try await Task.sleep(for: .milliseconds(2))
        }
        #expect(session.nativeSurfaceGeneration != 0)

        let output = (0 ..< 40)
            .map { String(format: "line-%02d", $0) }
            .joined(separator: "\r\n") + "\r\n"
        #expect(session.receive(Data(output.utf8)))
        var latest = ""
        for _ in 0 ..< 100 {
            try await Task.sleep(for: .milliseconds(2))
            latest = session.readViewportText() ?? ""
            if latest.contains("line-39") {
                break
            }
        }
        #expect(latest.contains("line-39"))
        #expect(!latest.contains("line-00"))

        let cgEvent = try #require(CGEvent(
            scrollWheelEvent2Source: nil,
            units: .line,
            wheelCount: 1,
            wheel1: 20,
            wheel2: 0,
            wheel3: 0,
        ))
        let event = try #require(NSEvent(cgEvent: cgEvent))
        view.scrollWheel(with: event)

        var scrolled = latest
        for _ in 0 ..< 100 {
            try await Task.sleep(for: .milliseconds(2))
            scrolled = session.readViewportText() ?? ""
            if scrolled != latest {
                break
            }
        }
        #expect(scrolled != latest)
        #expect(scrolled.contains("line-00") || !scrolled.contains("line-39"))
        #expect(writes.values.isEmpty)
    }

    @Test func `any scrollback setting keeps the terminal within its memory bound, in whole pages`() {
        let page = 512 * 1024
        let bound = 256 * 1024 * 1024
        for lines in [-5, 0, 1, 5000, 10000, 200_000, Int.max] {
            let bytes = TerminalScrollbackBudget(lines: lines).bytes
            #expect((2 * page ... bound).contains(bytes), "\(lines)")
            #expect(bytes % page == 0, "\(lines)")
        }
        #expect(TerminalScrollbackBudget(lines: 10000).bytes >= 10000 * 2400)
        #expect(TerminalScrollbackBudget(lines: .max).bytes == bound)
    }
#endif
