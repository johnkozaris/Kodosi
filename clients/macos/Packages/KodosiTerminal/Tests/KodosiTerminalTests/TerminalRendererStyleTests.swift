import AppKit
@testable import KodosiTerminal
import SwiftUI
import Testing

@Test @MainActor func hostedSwiftUIViewAppliesStyleWithoutReplacingSession() async throws {
    let session = TerminalRendererSession(write: { _ in }, resize: { _, _ in })
    let palette = TerminalPalette(
        background: "000000", foreground: "ffffff", cursor: "ffffff",
        selectionBackground: "333333", selectionForeground: "ffffff", ansiColors: Array(repeating: "000000", count: 16)
    )
    func style(_ fontSize: Float) -> TerminalStyle {
        TerminalStyle(
            fontFamily: "monospace", fontSize: fontSize, lineHeightAdjustment: 0, cursorStyle: .block,
            cursorBlink: false, scrollback: TerminalScrollbackBudget(lines: 10000), paddingX: 0, paddingY: 0,
            minimumContrast: 1, palette: palette
        )
    }
    session.setStyle(style(14))
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 500, height: 300),
        styleMask: [.titled], backing: .buffered, defer: false
    )
    let hosting = NSHostingView(rootView: TerminalRendererView(
        session: session, sessionId: "style-test", sessionName: "Style Test", accessibilityIdentifier: "test.terminal.style",
        shouldFocus: false, allowsFocus: false, allowsInput: false, isSurfaceVisible: true,
        onFocus: {}, onFocusChange: { _ in }, onShareShortcut: {}
    ))
    window.contentView = hosting
    window.orderBack(nil)
    defer { window.contentView = nil; window.orderOut(nil) }
    for _ in 0 ..< 100 where session.currentViewport == nil {
        try await Task.sleep(for: .milliseconds(10))
    }
    let initial = try #require(session.currentViewport)
    let generation = session.nativeSurfaceGeneration
    session.setStyle(style(24))
    for _ in 0 ..< 100 where session.currentViewport?.cellHeightPixels == initial.cellHeightPixels {
        try await Task.sleep(for: .milliseconds(10))
    }
    let larger = try #require(session.currentViewport)
    #expect(larger.cellHeightPixels > initial.cellHeightPixels)
    #expect(session.nativeSurfaceGeneration == generation)
    session.setStyle(style(14))
    for _ in 0 ..< 100 where session.currentViewport?.cellHeightPixels != initial.cellHeightPixels {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(session.currentViewport?.cellHeightPixels == initial.cellHeightPixels)
    #expect(session.nativeSurfaceGeneration == generation)
}
