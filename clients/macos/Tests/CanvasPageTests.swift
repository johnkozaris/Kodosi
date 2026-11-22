import AppKit
import Foundation
@testable import KodosiDesign
import SwiftUI
import Testing
import WebKit

@Test func `a canvas page opens the links that a person clicks in it, not those that its scripts click`() async throws {
    let frame = CanvasFrame()
    var opened: [String] = []
    frame.links.open = OpenURLAction { url in
        opened.append(url.absoluteString)
        return .handled
    }
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 240), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = NSHostingView(rootView: WebView(frame.page))
    defer { window.close() }
    try await show(#"<p style="position: fixed; inset: 0">An earlier page</p>"#, in: frame, window: window)
    let page = try #require(webView(in: window.contentView))
    click(page)
    try await show("""
    <a id="script" href="https://example.com/script">script</a>
    <a href="https://example.com/person" style="position: fixed; inset: 0">person</a>
    <script>document.getElementById("script").click()</script>
    """, in: frame, window: window)
    _ = try await frame.page.callJavaScript("document.getElementById('script').click()")
    click(page)
    for _ in 0 ..< 500 where opened.isEmpty {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(opened == ["https://example.com/person"])
}

@Test func `a canvas page ends its web view and its scripts when its view leaves`() async throws {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 240), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    defer { window.close() }
    window.contentView = NSHostingView(rootView: CanvasPageView(key: "page", frames: CanvasFrames(), content: Data("<p>A page</p>".utf8),
                                                                otherComputer: nil))
    weak var page: WKWebView?
    weak var scripts: WKUserContentController?
    for _ in 0 ..< 500 where page?.url == nil {
        window.contentView?.layoutSubtreeIfNeeded()
        page = webView(in: window.contentView)
        scripts = page?.configuration.userContentController
        try await Task.sleep(for: .milliseconds(10))
    }
    try #require(page?.url != nil)
    window.contentView = nil
    for _ in 0 ..< 500 where page != nil || scripts != nil {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(page == nil)
    #expect(scripts == nil)
}

private func show(_ html: String, in frame: CanvasFrame, window: NSWindow) async throws {
    await frame.show(Data(html.utf8))
    window.contentView?.layoutSubtreeIfNeeded()
    for _ in 0 ..< 500 where try await frame.page.callJavaScript("return innerWidth") as? Double ?? 0 == 0 {
        try await Task.sleep(for: .milliseconds(10))
    }
}

private func webView(in view: NSView?) -> WKWebView? {
    guard let view else { return nil }
    return view as? WKWebView ?? view.subviews.lazy.compactMap { webView(in: $0) }.first
}

private func click(_ view: NSView) {
    let location = view.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
    for (type, pressure) in [(NSEvent.EventType.leftMouseDown, Float(1)), (.leftMouseUp, 0)] {
        guard let event = NSEvent.mouseEvent(with: type, location: location, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                             windowNumber: view.window?.windowNumber ?? 0, context: nil, eventNumber: 0, clickCount: 1,
                                             pressure: pressure)
        else { continue }
        if type == .leftMouseDown {
            view.mouseDown(with: event)
        } else {
            view.mouseUp(with: event)
        }
    }
}
