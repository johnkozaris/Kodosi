import AppKit
import Foundation
@testable import KodosiDesign
import QuartzCore
import SwiftUI
import Testing

@MainActor
private func hosted(_ frame: NSRect = NSRect(x: 10, y: 10, width: 46, height: 46)) -> (NSWindow, LayerHostView) {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 160, height: 160), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    let host = LayerHostView(frame: frame)
    window.contentView?.addSubview(host)
    return (window, host)
}

@MainActor
private func lights(_ art: TurningLight, in host: LayerHostView) throws -> [(mask: CAShapeLayer, gradient: CAGradientLayer)] {
    host.update(setup: art, environment: EnvironmentValues()) { TurningLight.draw(art, $0) }
    return try #require(host.content.sublayers).map { holder in
        try (#require(holder.mask as? CAShapeLayer), #require(holder.sublayers?.first as? CAGradientLayer))
    }
}

@Test @MainActor func aWorkingLightTurnsWithTheClockAndRestsAtTheTopLeftWhenMotionIsReduced() throws {
    let (window, host) = hosted()
    defer { window.close() }
    let working = try lights(TurningLight(cornerRadius: 12, lineWidth: 2, light: .orange, core: nil, still: false), in: host)
    let rim = try #require(working.first)
    #expect(working.count == 1)
    #expect(rim.mask.fillColor == nil && rim.mask.lineWidth == 2)
    #expect(rim.gradient.type == .conic)
    let spin = try #require(rim.gradient.animation(forKey: "turn") as? CABasicAnimation)
    #expect(spin.keyPath == "transform.rotation.z")
    #expect(spin.duration == TurningLight.period)
    let gap = abs(spin.timeOffset - loopPhase(TurningLight.period))
    #expect(min(gap, TurningLight.period - gap) < 0.5)

    let still = try #require(try lights(TurningLight(cornerRadius: 12, lineWidth: 2, light: .orange, core: nil, still: true), in: host).first)
    #expect(still.gradient.animationKeys() == nil)
    #expect(abs(atan2(still.gradient.affineTransform().b, still.gradient.affineTransform().a) - TurningLight.rest) < 0.001)
}

@Test @MainActor func onATileTheLightAlsoWarmsTheFaceOnItsSide() throws {
    let (window, host) = hosted()
    defer { window.close() }
    let tile = try lights(TurningLight(cornerRadius: 12, lineWidth: 2, light: .orange, core: .white, still: false), in: host)
    #expect(tile.count == 2)
    #expect(tile[0].mask.fillColor != nil && tile[0].gradient.type == .radial)
    #expect(tile[1].mask.fillColor == nil && tile[1].gradient.type == .conic)
    #expect(tile.allSatisfy { $0.gradient.animation(forKey: "turn") != nil })
}

@Test func theLightLeadsWithAShortEdgeAndTrailsALongerOne() {
    #expect(TurningLight.glow(at: 0) == 1)
    #expect(abs(TurningLight.glow(at: 20.0 / 360) - exp(-0.5)) < 0.001)
    #expect(abs(TurningLight.glow(at: 1 - 36.0 / 360) - exp(-0.5)) < 0.001)
    #expect(TurningLight.glow(at: 0.5) < 0.001)
}

@Test func progressSweepsClockwiseFromTheTop() {
    let rect = CGRect(x: 0, y: 0, width: 40, height: 40)
    let quarter = Wedge(fraction: 0.25).path(in: rect)
    #expect(quarter.contains(CGPoint(x: 36, y: 4)))
    #expect(!quarter.contains(CGPoint(x: 4, y: 4)))
    #expect(!quarter.contains(CGPoint(x: 36, y: 36)))
    #expect(Wedge(fraction: 1).path(in: rect).contains(CGPoint(x: 4, y: 4)))
}

@Test @MainActor func aLineGlidesOnlyWhileItFlowsWithMotion() throws {
    let (window, host) = hosted(NSRect(x: 10, y: 10, width: 120, height: 3))
    defer { window.close() }
    for still in [false, true] {
        let art = GlidingLight(light: .orange, still: still)
        host.update(setup: art, environment: EnvironmentValues()) { GlidingLight.draw(art, $0) }
        let band = try #require(host.content.sublayers?.first as? CAGradientLayer)
        #expect((band.animation(forKey: "glide") != nil) == !still)
    }
}

@Test @MainActor func loopingMarksAnimateInCoreAnimationAndStopWithoutAWindow() throws {
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
