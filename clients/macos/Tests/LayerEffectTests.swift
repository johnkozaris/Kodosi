import AppKit
import Foundation
@testable import KodosiDesign
import QuartzCore
import SwiftUI
import Testing

private func hosted(_ frame: NSRect = NSRect(x: 10, y: 10, width: 46, height: 46)) -> (NSWindow, LayerHostView) {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 160, height: 160), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    let host = LayerHostView(frame: frame)
    window.contentView?.addSubview(host)
    return (window, host)
}

@Test func `looping marks animate in core animation and stop without a window`() throws {
    let (window, host) = hosted(NSRect(x: 10, y: 10, width: 18, height: 18))
    let breath = Breath(color: .orange, size: 7)
    host.update(setup: breath, environment: EnvironmentValues()) { Breath.draw(breath, $0) }
    #expect(host.content.sublayers?.first?.animation(forKey: "breath") != nil)

    let typing = Cursor(color: .orange, radius: 2, blink: .typing)
    host.update(setup: typing, environment: EnvironmentValues()) { Cursor.draw(typing, $0) }
    let blink = try #require(host.content.sublayers?.first?.animation(forKey: "blink"))
    #expect(blink.beginTime > CACurrentMediaTime())

    let sweep = Sweep(color: .white, radius: 5)
    host.update(setup: sweep, environment: EnvironmentValues()) { Sweep.draw(sweep, $0) }
    #expect(host.content.masksToBounds)
    #expect(host.content.sublayers?.first?.animation(forKey: "sweep") != nil)

    host.removeFromSuperview()
    window.close()
    #expect(host.content.sublayers == nil)
    let fading = Cursor(color: .orange, radius: 2, blink: .fading)
    host.update(setup: fading, environment: EnvironmentValues()) { Cursor.draw(fading, $0) }
    #expect(host.content.sublayers == nil)
}
