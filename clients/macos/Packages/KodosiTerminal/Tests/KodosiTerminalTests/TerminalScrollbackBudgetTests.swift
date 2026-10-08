import AppKit
@testable import KodosiTerminal
import SwiftUI
import Testing

private final class LockedByteRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [Data] = []

    var values: [Data] {
        lock.withLock { storage }
    }

    func append(_ value: Data) {
        lock.withLock { storage.append(value) }
    }
}

@MainActor
private final class AccessibilityUpdateRecorder {
    private(set) var count = 0

    func record() {
        count += 1
    }

    func recordAndReturnCount() -> Int {
        count += 1
        return count
    }
}

@Test func scrollbackBudgetUsesExplicitBoundedByteUnits() {
    let minimum = TerminalScrollbackBudget(lines: 100)
    let standard = TerminalScrollbackBudget(lines: 10000)
    let maximum = TerminalScrollbackBudget(lines: 100_000)

    #expect(minimum.bytes == TerminalScrollbackBudget.minimumBytes)
    #expect(standard.bytes == 46 * TerminalScrollbackBudget.allocationPageBytes)
    #expect(standard.bytes > standard.lines)
    #expect(maximum.bytes > standard.bytes)
    #expect(maximum.bytes <= TerminalScrollbackBudget.maximumBytes)
    #expect(minimum.bytes.isMultiple(of: TerminalScrollbackBudget.allocationPageBytes))
    #expect(standard.bytes.isMultiple(of: TerminalScrollbackBudget.allocationPageBytes))
    #expect(maximum.bytes.isMultiple(of: TerminalScrollbackBudget.allocationPageBytes))
}

@Test func scrollbackBudgetClampsOverflowToMaximum() {
    #expect(TerminalScrollbackBudget(lines: .max).bytes == TerminalScrollbackBudget.maximumBytes)
}

@Test func hostManagedRendererDeniesClipboardProtocolsAndImages() {
    #expect(TerminalRendererSession.hostManagedPolicyOverrides.map(\.key) == [
        "clipboard-read",
        "clipboard-write",
        "image-storage-limit",
        "mouse-shift-capture",
    ])
    #expect(TerminalRendererSession.hostManagedPolicyOverrides.map(\.value) == [
        "deny",
        "deny",
        "0",
        "never",
    ])
}

@MainActor
@Test func accessibilitySubscribersRemainIndependent() async {
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { _ in },
        resize: { _, _ in }
    )
    let first = AccessibilityUpdateRecorder()
    let second = AccessibilityUpdateRecorder()
    let firstToken = session.registerAccessibilityUpdateHandler {
        first.record()
        return true
    }
    let secondToken = session.registerAccessibilityUpdateHandler {
        second.record()
        return true
    }

    session.signalAccessibilityViewportChange()
    try? await Task.sleep(for: .milliseconds(150))
    #expect(first.count == 1)
    #expect(second.count == 1)

    session.unregisterAccessibilityUpdateHandler(firstToken)
    session.signalAccessibilityViewportChange()
    try? await Task.sleep(for: .milliseconds(150))
    #expect(first.count == 1)
    #expect(second.count == 2)
    session.unregisterAccessibilityUpdateHandler(secondToken)
}

@MainActor
@Test func accessibilityRefreshRetriesUntilCommittedViewportChanges() async {
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { _ in },
        resize: { _, _ in }
    )
    let attempts = AccessibilityUpdateRecorder()
    let token = session.registerAccessibilityUpdateHandler {
        attempts.recordAndReturnCount() == 3
    }

    session.signalAccessibilityViewportChange()
    for _ in 0 ..< 20 where attempts.count < 3 {
        try? await Task.sleep(for: .milliseconds(20))
    }

    #expect(attempts.count == 3)
    session.unregisterAccessibilityUpdateHandler(token)
}

@MainActor
@Test func nativeSurfaceGenerationIdentifiesFirstAndReplacementAttachments() {
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { _ in },
        resize: { _, _ in }
    )

    #expect(session.nativeSurfaceGeneration == 0)

    session.viewState.onSurfaceAttached?()
    #expect(session.nativeSurfaceGeneration != 0)
    #expect(session.nativeSurfaceGeneration == 1)

    session.viewState.onSurfaceAttached?()
    #expect(session.nativeSurfaceGeneration == 2)
}

@MainActor
@Test func rendererSessionDeallocatesAfterOwnerRelease() {
    weak var weakSession: TerminalRendererSession?
    do {
        let session = TerminalRendererSession(
            colorScheme: .dark,
            write: { _ in },
            resize: { _, _ in }
        )
        weakSession = session
    }

    #expect(weakSession == nil)
}

@MainActor
@Test func readOnlyRendererSuppressesBackendWrites() {
    let writes = LockedByteRecorder()
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { writes.append($0) },
        resize: { _, _ in }
    )
    session.setAllowsInput(false)

    session.sendInputForTest(Data("blocked".utf8))
    #expect(writes.values.isEmpty)

    session.setAllowsInput(true)
    session.sendInputForTest(Data("allowed".utf8))
    #expect(writes.values == [Data("allowed".utf8)])
}

@MainActor
@Test func runtimeFocusRevocationResignsAndAllowsUserReacquisition() {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 400, height: 240),
        styleMask: [.titled],
        backing: .buffered,
        defer: false
    )
    let view = TerminalRendererHostedView(frame: window.contentView?.bounds ?? .zero)
    var focusEvents: [Bool] = []
    view.onFocusChange = { focusEvents.append($0) }
    window.contentView = view
    window.makeKeyAndOrderFront(nil)

    #expect(window.makeFirstResponder(view))
    #expect(window.firstResponder === view)
    view.revokeNativeFocus()
    #expect(window.firstResponder !== view)
    #expect(!view.canSynchronizeRequestedFocus)

    #expect(window.makeFirstResponder(view))
    #expect(window.firstResponder === view)
    #expect(view.canSynchronizeRequestedFocus)
    #expect(focusEvents.filter(\.self).count == (window.isKeyWindow ? 2 : 0))
    window.orderOut(nil)
}

@MainActor
@Test func windowKeyLossReleasesFocusWithoutResponderReplacement() {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 240),
                          styleMask: [.titled], backing: .buffered, defer: false)
    let view = TerminalRendererHostedView(frame: window.contentView?.bounds ?? .zero)
    var focusEvents: [Bool] = []
    view.onFocusChange = { focusEvents.append($0) }
    window.contentView = view
    window.makeKeyAndOrderFront(nil)
    defer { window.contentView = nil; window.orderOut(nil) }
    window.makeFirstResponder(view)
    #expect(window.firstResponder === view)
    focusEvents.removeAll()
    NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
    #expect(focusEvents == [false])
    #expect(window.firstResponder === view)
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
        keyCode: keyCode
    ))
}

@MainActor
@Test func readOnlyHostedViewConsumesInputProducingEvents() async throws {
    let writes = LockedByteRecorder()
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { writes.append($0) },
        resize: { _, _ in }
    )
    session.setAllowsInput(false)
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 400, height: 240),
        styleMask: [.titled],
        backing: .buffered,
        defer: false
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
        pressure: 0
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
@Test func hostedViewExposesViewportTextAccessibility() {
    let view = TerminalRendererHostedView(frame: .zero)
    view.viewportText = { "visible terminal output" }
    view.refreshAccessibilityViewport()

    #expect(view.accessibilityValue() as? String == "visible terminal output")
    #expect(view.accessibilityVisibleCharacterRange() == NSRange(location: 0, length: 23))
    #expect(view.accessibilityNumberOfCharacters() == 23)
    #expect(view.accessibilityString(for: NSRange(location: 8, length: 8)) == "terminal")
    #expect(view.accessibilityString(for: NSRange(location: 22, length: 2)) == nil)
    #expect(view.accessibilityString(for: NSRange(location: NSNotFound, length: 0)) == nil)
}

@MainActor
@Test func accessibilityTextQueriesRefreshOneCachedViewport() {
    let view = TerminalRendererHostedView(frame: .zero)
    var text = "old"
    view.viewportText = { text }
    view.refreshAccessibilityViewport()
    text = "new terminal output"

    #expect(view.accessibilityNumberOfCharacters() == 19)
    #expect(view.accessibilityVisibleCharacterRange() == NSRange(location: 0, length: 19))
    #expect(view.accessibilityString(for: NSRange(location: 4, length: 8)) == "terminal")
    #expect(view.accessibilityValue() as? String == "new terminal output")
}

@MainActor
@Test func readOnlyHostedViewScrollsViewportWithoutBackendWrites() async throws {
    let writes = LockedByteRecorder()
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { writes.append($0) },
        resize: { _, _ in }
    )
    session.setAllowsInput(false)
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 400, height: 160),
        styleMask: [.titled],
        backing: .buffered,
        defer: false
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
        wheel3: 0
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

@MainActor
@Test func rendererAppliesScrollbackBudgetToSurfaceConfiguration() {
    let session = TerminalRendererSession(
        colorScheme: .dark,
        write: { _ in },
        resize: { _, _ in }
    )
    let palette = TerminalPalette(
        background: "000000",
        foreground: "ffffff",
        cursor: "ffffff",
        selectionBackground: "333333",
        selectionForeground: "ffffff",
        ansiColors: Array(repeating: "000000", count: 16)
    )
    let style = TerminalStyle(
        fontFamily: "monospace",
        fontSize: 14,
        lineHeightAdjustment: 0,
        cursorStyle: .block,
        cursorBlink: false,
        scrollback: TerminalScrollbackBudget(lines: 10000),
        paddingX: 0,
        paddingY: 0,
        minimumContrast: 1,
        lightPalette: palette,
        darkPalette: palette
    )

    session.setStyle(style)

    #expect(session.viewState.configuration.scrollbackLimitBytes == style.scrollback.bytes)
}
