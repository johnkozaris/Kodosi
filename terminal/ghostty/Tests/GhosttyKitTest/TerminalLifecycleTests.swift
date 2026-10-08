import AppKit
import Foundation
import GhosttyKit
@testable import GhosttyTerminal
import Testing

@MainActor
@Suite(.serialized)
struct TerminalLifecycleTests {
    @Test
    func `semantic checkpoint restores a live host managed surface transactionally`() async {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        view.wantsLayer = true
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        let restoreInProgress = LockedFlag()
        var selectionChanges = 0
        var callbacksDuringRestore = 0
        bridge.onSelectionChanged = {
            if restoreInProgress.value {
                callbacksDuringRestore += 1
            }
            selectionChanges += 1
        }
        guard let surface = controller.createSurface(
            bridge: bridge,
            configuration: TerminalSurfaceOptions(session: session),
            platformSetup: { config in
                config.platform_tag = GHOSTTY_PLATFORM_MACOS
                config.platform = ghostty_platform_u(
                    macos: ghostty_platform_macos_s(
                        nsview: Unmanaged.passUnretained(view).toOpaque()
                    )
                )
                config.scale_factor = 1
            }
        ) else {
            Issue.record("failed to create host-managed visual surface")
            return
        }
        session.setSurface(surface)
        defer {
            session.clearSurface(ifMatches: surface)
            ghostty_surface_free(surface)
            controller.remove(bridge)
        }

        guard let fixtureURL = Bundle.module.url(
            forResource: "visual-checkpoint",
            withExtension: "json"
        ), let checkpoint = try? Data(contentsOf: fixtureURL) else {
            Issue.record("missing visual checkpoint fixture")
            return
        }
        let initial = Data("select me".utf8)
        #expect(session.receive(initial))
        session.waitForPendingOutput()
        #expect("select_all".withCString {
            ghostty_surface_binding_action(surface, $0, UInt("select_all".utf8.count))
        })
        #expect(ghostty_surface_has_selection(surface))
        try? await Task.sleep(for: .milliseconds(20))
        selectionChanges = 0
        restoreInProgress.value = true
        #expect(session.restoreCheckpointSynchronously(checkpoint))
        restoreInProgress.value = false
        #expect(callbacksDuringRestore == 0)
        #expect(!ghostty_surface_has_selection(surface))
        let size = ghostty_surface_size(surface)
        #expect(size.columns == 8)
        #expect(size.rows == 2)
        let viewportText = session.readViewportText()
        #expect(viewportText?.filter { !$0.isWhitespace }.contains("ALT") == true, "Restored viewport: \(viewportText ?? "nil")")
        controller.handleWakeup()
        try? await Task.sleep(for: .milliseconds(20))
        #expect(selectionChanges == 1)

        let beforeSize = ghostty_surface_size(surface)
        #expect(!session.restoreCheckpointSynchronously(Data("{".utf8)))
        controller.handleWakeup()
        try? await Task.sleep(for: .milliseconds(20))
        #expect(selectionChanges == 1)
        let afterSize = ghostty_surface_size(surface)
        #expect(afterSize.columns == beforeSize.columns)
        #expect(afterSize.rows == beforeSize.rows)

        #expect(session.restoreCheckpointSynchronously(checkpoint))
        #expect(callbacksDuringRestore == 0)
    }

    @Test
    func `surface initialization and checkpoint restore retain destination scrollback`() throws {
        let fixtureURL = try #require(Bundle.module.url(forResource: "visual-checkpoint", withExtension: "json"))
        let checkpoint = try Data(contentsOf: fixtureURL)
        for budget in [0, 64 * 1024 * 1024] {
            let view = NSView(frame: NSRect(x: 0, y: 0, width: 160, height: 80))
            view.wantsLayer = true
            let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
            let controller = TerminalController(configuration: .init { $0.withCustom("scrollback-limit", "0") })
            let bridge = TerminalCallbackBridge()
            let surface = try #require(controller.createSurface(
                bridge: bridge,
                configuration: TerminalSurfaceOptions(session: session, scrollbackLimitBytes: budget),
                platformSetup: { config in
                    config.platform_tag = GHOSTTY_PLATFORM_MACOS
                    config.platform = ghostty_platform_u(macos: ghostty_platform_macos_s(
                        nsview: Unmanaged.passUnretained(view).toOpaque()
                    ))
                    config.scale_factor = 1
                }
            ))
            session.setSurface(surface)
            defer {
                session.clearSurface(ifMatches: surface)
                ghostty_surface_free(surface)
                controller.remove(bridge)
            }
            for restore in [false, true] {
                if restore {
                    #expect(session.restoreCheckpointSynchronously(checkpoint))
                }
                #expect(session.receive(Data("\u{1B}[?1049l\u{1B}[2J\u{1B}[3J\u{1B}[H".utf8)))
                let lines = (0 ..< 12000).map { "\($0)\r\n" }.joined()
                #expect(session.receive(Data(lines.utf8)))
                session.waitForPendingOutput()
                #expect("select_all".withCString { ghostty_surface_binding_action(surface, $0, 10) })
                var selected = ghostty_text_s()
                #expect(ghostty_surface_read_selection(surface, &selected))
                defer { ghostty_surface_free_text(surface, &selected) }
                let count = selected.text.map { pointer in
                    String(decoding: UnsafeRawBufferPointer(start: pointer, count: Int(selected.text_len)), as: UTF8.self)
                        .split(separator: "\n").count
                } ?? 0
                if budget == 0 {
                    #expect(count <= Int(ghostty_surface_size(surface).rows))
                } else {
                    #expect(count >= 12000)
                }
            }
        }
    }

    @Test
    func `tearDownSurface releases AppKit native surface`() {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let view = TerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 640, height: 480),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        view.controller = controller
        view.configuration = TerminalSurfaceOptions(session: session)
        window.contentView = view
        view.core.rebuildIfReady()

        #expect(session.currentSurface != nil)
        #expect(controller.retainedBridgeCount == 1)

        view.tearDownSurface()

        #expect(session.currentSurface == nil)
        #expect(controller.retainedBridgeCount == 0)
    }

    @Test
    func `host managed parser reports stay native while literal user bytes pass through`() {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        view.wantsLayer = true
        let writes = LockedTerminalWrites()
        let session = InMemoryTerminalSession(
            write: { writes.append($0) },
            resize: { _ in }
        )
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        guard let surface = controller.createSurface(
            bridge: bridge,
            configuration: TerminalSurfaceOptions(session: session),
            platformSetup: { config in
                config.platform_tag = GHOSTTY_PLATFORM_MACOS
                config.platform = ghostty_platform_u(
                    macos: ghostty_platform_macos_s(
                        nsview: Unmanaged.passUnretained(view).toOpaque()
                    )
                )
                config.scale_factor = 1
            }
        ) else {
            Issue.record("failed to create host-managed query surface")
            return
        }
        session.setSurface(surface)
        defer {
            session.clearSurface(ifMatches: surface)
            ghostty_surface_free(surface)
            controller.remove(bridge)
        }

        #expect(session.receive(Data("\u{1B}[6n\u{1B}[?1004$p\u{1B}P$qm\u{1B}\\".utf8)))
        session.waitForPendingOutput()
        #expect(writes.values.isEmpty)

        let literal = Data("\u{1B}[I\u{1B}[O\u{1B}[?1004;1$y\u{1B}P1$r0m\u{1B}\\".utf8)
        session.sendInput(literal)
        #expect(writes.values == [literal])
    }

    @Test
    func `host managed clipboard callbacks admit only user paste`() {
        #expect(terminalClipboardRequestAllowed(
            request: GHOSTTY_CLIPBOARD_REQUEST_PASTE
        ))
        #expect(!terminalClipboardRequestAllowed(
            request: GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ
        ))
        #expect(!terminalClipboardRequestAllowed(
            request: GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE
        ))
    }

    @Test
    func `host managed paste stays direct when mode 5522 is set`() {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        view.wantsLayer = true
        let writes = LockedTerminalWrites()
        let session = InMemoryTerminalSession(
            write: { writes.append($0) },
            resize: { _ in }
        )
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        guard let rawSurface = controller.createSurface(
            bridge: bridge,
            configuration: TerminalSurfaceOptions(session: session),
            platformSetup: { config in
                config.platform_tag = GHOSTTY_PLATFORM_MACOS
                config.platform = ghostty_platform_u(
                    macos: ghostty_platform_macos_s(
                        nsview: Unmanaged.passUnretained(view).toOpaque()
                    )
                )
                config.scale_factor = 1
            }
        ) else {
            Issue.record("failed to create host-managed paste surface")
            return
        }
        bridge.rawSurface = rawSurface
        session.setSurface(rawSurface)
        defer {
            session.clearSurface(ifMatches: rawSurface)
            ghostty_surface_free(rawSurface)
            controller.remove(bridge)
        }

        #expect(session.receive(Data("\u{1B}[?5522h".utf8)))
        session.waitForPendingOutput()

        let pasteboard = NSPasteboard.general
        let previous = pasteboard.string(forType: .string)
        pasteboard.clearContents()
        #expect(pasteboard.setString("paste-event-policy", forType: .string))
        defer {
            pasteboard.clearContents()
            if let previous {
                pasteboard.setString(previous, forType: .string)
            }
        }

        let surface = TerminalSurface(rawSurface)
        #expect(surface.performBindingAction("paste_from_clipboard"))
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        #expect(writes.values == [Data("paste-event-policy".utf8)])
    }

    @Test
    func `host managed clipboard reads require canonical plain text`() {
        "text/plain".withCString { plainText in
            let supported: [UnsafePointer<CChar>?] = [plainText]
            supported.withUnsafeBufferPointer { mimeTypes in
                #expect(terminalClipboardRequestsPlainText(
                    mimeTypes: mimeTypes.baseAddress,
                    count: mimeTypes.count
                ))
            }
        }
        "application/json".withCString { json in
            let unsupported: [UnsafePointer<CChar>?] = [json]
            unsupported.withUnsafeBufferPointer { mimeTypes in
                #expect(!terminalClipboardRequestsPlainText(
                    mimeTypes: mimeTypes.baseAddress,
                    count: mimeTypes.count
                ))
            }
        }
        #expect(!terminalClipboardRequestsPlainText(mimeTypes: nil, count: 0))
    }

    @Test
    func `host managed clipboard writes fail closed`() {
        #expect(terminalClipboardWriteAllowed(
            clipboard: GHOSTTY_CLIPBOARD_STANDARD,
            requiresConfirmation: false
        ))
        #expect(!terminalClipboardWriteAllowed(
            clipboard: GHOSTTY_CLIPBOARD_STANDARD,
            requiresConfirmation: true
        ))
        #expect(!terminalClipboardWriteAllowed(
            clipboard: GHOSTTY_CLIPBOARD_SELECTION,
            requiresConfirmation: false
        ))
        #expect(!terminalClipboardWriteAllowed(
            clipboard: GHOSTTY_CLIPBOARD_PRIMARY,
            requiresConfirmation: false
        ))
    }

    @Test
    func `display ID extraction requires a nonzero screen number`() {
        let key = NSDeviceDescriptionKey("NSScreenNumber")

        #expect(AppTerminalView.displayID(in: [key: NSNumber(value: UInt32.max)]) == UInt32.max)
        #expect(AppTerminalView.displayID(in: [key: NSNumber(value: 0)]) == nil)
        #expect(AppTerminalView.displayID(in: [key: "1"]) == nil)
        #expect(AppTerminalView.displayID(in: [:]) == nil)
    }

    @Test
    func `surface creation forwards display ID before initial metrics`() {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let coordinator = TerminalSurfaceCoordinator()
        let hostView = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        hostView.wantsLayer = true
        let window = NSWindow(
            contentRect: hostView.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostView
        var routedSurface: TerminalSurface?
        var routedDisplayID: UInt32?
        var events: [String] = []

        coordinator.isAttached = { hostView.window != nil }
        coordinator.displayIDProvider = { 120 }
        coordinator.scaleFactor = { 1 }
        coordinator.viewSize = { (hostView.bounds.width, hostView.bounds.height) }
        coordinator.platformSetup = { config in
            config.platform_tag = GHOSTTY_PLATFORM_MACOS
            config.platform = ghostty_platform_u(
                macos: ghostty_platform_macos_s(
                    nsview: Unmanaged.passUnretained(hostView).toOpaque()
                )
            )
        }
        coordinator.applyDisplayID = { surface, displayID in
            events.append("display")
            routedSurface = surface
            routedDisplayID = displayID
        }
        coordinator.onMetricsUpdate = {
            events.append("metrics")
        }
        coordinator.configuration = .init(session: session)
        coordinator.controller = controller
        defer {
            coordinator.freeSurface()
            window.contentView = nil
        }

        #expect(routedSurface === coordinator.surface)
        #expect(routedDisplayID == 120)
        #expect(events.first == "display")
        #expect(events.contains("metrics"))
        #expect(coordinator.surface?.size() != nil)
    }

    @Test
    func `zero size defers configuration rebuild until layout recovers`() {
        let firstSession = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let replacementSession = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let coordinator = TerminalSurfaceCoordinator()
        let hostView = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        hostView.wantsLayer = true
        let window = NSWindow(
            contentRect: hostView.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostView
        var viewSize = (width: 640.0, height: 480.0)

        coordinator.isAttached = { hostView.window != nil }
        coordinator.scaleFactor = { 1 }
        coordinator.viewSize = { viewSize }
        coordinator.platformSetup = { config in
            config.platform_tag = GHOSTTY_PLATFORM_MACOS
            config.platform = ghostty_platform_u(
                macos: ghostty_platform_macos_s(
                    nsview: Unmanaged.passUnretained(hostView).toOpaque()
                )
            )
        }
        coordinator.configuration = .init(session: firstSession)
        coordinator.controller = controller
        defer {
            coordinator.freeSurface()
            window.contentView = nil
        }

        let initialSurface = coordinator.surface
        let initialRawSurface = initialSurface?.rawValue
        #expect(initialRawSurface != nil)
        #expect(firstSession.currentSurface == initialRawSurface)

        viewSize = (0, 0)
        coordinator.configuration = .init(session: replacementSession)
        #expect(coordinator.surface === initialSurface)
        #expect(firstSession.currentSurface == initialRawSurface)
        #expect(replacementSession.currentSurface == nil)

        coordinator.fitToSize()
        #expect(coordinator.surface === initialSurface)

        viewSize = (640, 480)
        coordinator.fitToSize()
        #expect(coordinator.surface !== initialSurface)
        #expect(firstSession.currentSurface == nil)
        #expect(replacementSession.currentSurface == coordinator.surface?.rawValue)
    }

    @Test
    func `screen changes retarget the existing surface and ignore other windows`() async {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let view = AppTerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        let window = NSWindow(
            contentRect: view.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        let otherWindow = NSWindow(
            contentRect: view.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        var displayID: UInt32? = 60
        var routed: [(TerminalSurface, UInt32)] = []

        view.core.displayIDProvider = { displayID }
        view.core.applyDisplayID = { surface, displayID in
            routed.append((surface, displayID))
        }
        view.controller = controller
        view.configuration = .init(session: session)
        window.contentView = view
        defer {
            view.tearDownSurface()
            window.contentView = nil
        }

        let surface = view.surface
        #expect(surface != nil)
        #expect(routed.allSatisfy { $0.0 === surface && $0.1 == 60 })
        let attachedRouteCount = routed.count

        displayID = 120
        view.windowDidChangeScreen(Notification(
            name: NSWindow.didChangeScreenNotification,
            object: window
        ))
        #expect(routed.count == attachedRouteCount + 1)
        #expect(routed.last?.0 === surface)
        #expect(routed.last?.1 == 120)

        let currentWindowRouteCount = routed.count
        view.windowDidChangeScreen(Notification(
            name: NSWindow.didChangeScreenNotification,
            object: otherWindow
        ))
        #expect(routed.count == currentWindowRouteCount)
        await Task.yield()

        #expect(view.surface === surface)
    }

    @Test
    func `display synchronization routes every valid transition to the current surface`() {
        let coordinator = TerminalSurfaceCoordinator()
        let first = TerminalSurface(testLifecycleSurface(1))
        let replacement = TerminalSurface(testLifecycleSurface(2))
        var displayID: UInt32? = 60
        var routed: [(TerminalSurface, UInt32)] = []

        coordinator.displayIDProvider = { displayID }
        coordinator.applyDisplayID = { surface, displayID in
            routed.append((surface, displayID))
        }

        coordinator.synchronizeDisplayID(on: first)
        displayID = 120
        coordinator.synchronizeDisplayID(on: first)
        coordinator.synchronizeDisplayID(on: replacement)
        displayID = 0
        coordinator.synchronizeDisplayID(on: replacement)
        displayID = nil
        coordinator.synchronizeDisplayID(on: replacement)

        #expect(routed.count == 3)
        #expect(routed[0].0 === first)
        #expect(routed[0].1 == 60)
        #expect(routed[1].0 === first)
        #expect(routed[1].1 == 120)
        #expect(routed[2].0 === replacement)
        #expect(routed[2].1 == 120)
    }

    @Test
    func `failed surface creation does not retain bridge`() {
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })

        let surface = controller.createSurface(
            bridge: bridge,
            configuration: .init(session: session)
        ) { _ in }

        #expect(surface == nil)
        #expect(controller.retainedBridgeCount == 0)
    }

    @Test
    func `switching controllers removes bridge from old controller`() {
        let oldController = TerminalController()
        let newController = TerminalController()
        let coordinator = TerminalSurfaceCoordinator()

        coordinator.isAttached = { false }
        oldController.retain(coordinator.bridge)
        #expect(oldController.retainedBridgeCount == 1)

        coordinator.controller = oldController
        #expect(oldController.retainedBridgeCount == 0)

        oldController.retain(coordinator.bridge)
        #expect(oldController.retainedBridgeCount == 1)

        coordinator.controller = newController

        #expect(oldController.retainedBridgeCount == 0)
        #expect(newController.retainedBridgeCount == 0)
    }

    @Test
    func `free surface removes retained bridge`() {
        let controller = TerminalController()
        let coordinator = TerminalSurfaceCoordinator()

        coordinator.isAttached = { false }
        coordinator.controller = controller

        controller.retain(coordinator.bridge)
        #expect(controller.retainedBridgeCount == 1)

        coordinator.freeSurface()

        #expect(controller.retainedBridgeCount == 0)
    }

    @Test
    func `wakeup reaches every renderable surface`() {
        let controller = TerminalController()
        let suspended = TerminalCallbackBridge()
        let first = TerminalCallbackBridge()
        let second = TerminalCallbackBridge()
        var firstWakeups = 0
        var secondWakeups = 0

        suspended.canProcessAppWakeup = { false }
        suspended.onAppWakeup = { Issue.record("suspended surface received wakeup") }
        first.canProcessAppWakeup = { true }
        first.onAppWakeup = { firstWakeups += 1 }
        second.canProcessAppWakeup = { true }
        second.onAppWakeup = { secondWakeups += 1 }
        controller.retain(suspended)
        controller.retain(first)
        controller.retain(second)

        controller.handleWakeup()

        #expect(firstWakeups == 1)
        #expect(secondWakeups == 1)
    }

    @Test
    func `wakeup ticks application with no retained surfaces`() {
        let controller = TerminalController()
        var ticks = 0

        controller.handleWakeup {
            ticks += 1
        }

        #expect(ticks == 1)
    }

    @Test
    func `wakeup drains application while every surface is suspended`() {
        let controller = TerminalController()
        let first = TerminalCallbackBridge()
        let second = TerminalCallbackBridge()
        var appTicks = 0
        var surfaceWakeups = 0

        first.canProcessAppWakeup = { false }
        first.onAppWakeup = { surfaceWakeups += 1 }
        second.canProcessAppWakeup = { false }
        second.onAppWakeup = { surfaceWakeups += 1 }
        controller.retain(first)
        controller.retain(second)

        controller.handleWakeup {
            appTicks += 1
        }

        #expect(appTicks == 1)
        #expect(surfaceWakeups == 0)
    }

    @Test
    func `retaining the same bridge is idempotent`() {
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()

        controller.retain(bridge)
        controller.retain(bridge)

        #expect(controller.retainedBridgeCount == 1)
        controller.remove(bridge)
        #expect(controller.retainedBridgeCount == 0)
    }

    @Test
    func `backend replacement detaches the session that owns the surface`() {
        let first = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let second = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let coordinator = TerminalSurfaceCoordinator()
        let surface = testLifecycleSurface(1)

        coordinator.configuration = .init(session: first)
        coordinator.attachInMemorySession(to: surface)
        #expect(first.currentSurface == surface)

        coordinator.configuration = .init(session: second)
        coordinator.freeSurface()

        #expect(first.currentSurface == nil)
        #expect(second.currentSurface == nil)
    }

    @Test
    func `font and visual config changes update without rebuilding`() {
        let session = InMemoryTerminalSession(
            write: { _ in },
            resize: { _ in }
        )
        let base = TerminalSurfaceOptions(
            session: session,
            fontSize: 12,
            terminalConfiguration: .init {
                $0.withBackground("111111")
            },
            scrollbackLimitBytes: 4 * 1024 * 1024
        )
        var updated = base
        updated.fontSize = 18
        updated.terminalConfiguration = .init {
            $0.withBackground("222222")
        }

        #expect(!updated.isEquivalent(to: base))
        #expect(!updated.requiresSurfaceRebuild(comparedTo: base))
    }

    @Test
    func `scrollback settings update without replacing the surface`() {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let base = TerminalSurfaceOptions(session: session)
        var updated = base
        updated.scrollbackLimitBytes = 8 * 1024 * 1024

        #expect(!updated.isEquivalent(to: base))
        #expect(!updated.requiresSurfaceRebuild(comparedTo: base))
    }

    @Test
    func `surface config owns font and scrollback overrides`() {
        let controller = TerminalController(configuration: .init {
            $0.withFontSize(10)
        })
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let resolved = controller.resolvedSurfaceConfigContents(.init(
            session: session,
            fontSize: 17,
            terminalConfiguration: .init {
                $0.withBackground("123456")
            },
            scrollbackLimitBytes: 24 * 1024 * 1024
        ))

        #expect(resolved.contains("background = 123456"))
        #expect(resolved.contains("font-size = 17"))
        #expect(resolved.contains("scrollback-limit = 25165824"))
    }

    @Test
    func `surface config layers on controller theme`() {
        let theme = TerminalTheme(
            light: .init { $0.withForeground("111111") },
            dark: .init { $0.withForeground("eeeeee") }
        )
        let controller = TerminalController(
            configuration: .init { $0.withCursorStyleBlink(false) },
            theme: theme
        )

        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let inherited = controller.resolvedSurfaceConfigContents(.init(session: session))
        let overridden = controller.resolvedSurfaceConfigContents(.init(
            session: session,
            terminalConfiguration: .init {
                $0.withBackground("123456")
            }
        ))

        #expect(inherited == controller.renderedConfig)
        #expect(overridden.contains("cursor-style-blink = false"))
        #expect(overridden.contains("foreground = 111111"))
        #expect(overridden.contains("background = 123456"))
    }

    @Test
    func `application active state controls immediate ticks`() async {
        let coordinator = TerminalSurfaceCoordinator()
        var renders = 0

        coordinator.isAttached = { true }
        coordinator.onPostRender = {
            renders += 1
        }

        coordinator.setApplicationActive(false)
        coordinator.requestImmediateTick()
        await Task.yield()

        #expect(renders == 0)

        coordinator.setApplicationActive(true)
        await Task.yield()

        #expect(renders == 1)
    }
}

private final class LockedTerminalWrites: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [Data] = []

    var values: [Data] {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }

    func append(_ data: Data) {
        lock.lock()
        storage.append(data)
        lock.unlock()
    }
}

private func testLifecycleSurface(_ address: Int) -> ghostty_surface_t {
    UnsafeMutableRawPointer(bitPattern: address)!
}
