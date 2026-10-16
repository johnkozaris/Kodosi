import AppKit
import Foundation
@testable import KodosiDesign
import QuartzCore
import SwiftUI
import Testing

/// A reference that walks the keystrokes one by one: absolute step `a` falls at `time(a)` and types on cell `a mod lap`.
private struct Keystrokes {
    let total: Int
    let offsets: [TimeInterval]
    let period: TimeInterval

    init(lap: Int) {
        total = lap * Int((64 / Double(lap)).rounded(.up))
        var sum: TimeInterval = 0
        offsets = TypingCadence.gaps(total).map { gap in
            sum += gap
            return sum
        }
        period = sum
    }

    func time(_ step: Int) -> TimeInterval {
        let cycle = Int((Double(step) / Double(total)).rounded(.down))
        return Double(cycle) * period + offsets[step - cycle * total]
    }

    func latest(atOrBefore time: TimeInterval) -> Int {
        (-1 ..< total - 1).last { self.time($0) <= time } ?? -1
    }
}

private func positive(_ value: Int, _ modulus: Int) -> Int {
    ((value % modulus) + modulus) % modulus
}

@Test func aRimTypesEveryCellAndCoolsItLikeTheCanvasDid() {
    for count in [8, 18, 21, 23, 64, 70] {
        let frames = CellTrackFrames.rim(count: count)
        let keys = Keystrokes(lap: count)
        #expect(abs(frames.period - keys.period) < 1e-9)
        for sample in 0 ..< 240 {
            let time = keys.period * (Double(sample) + 0.37) / 240
            let head = keys.latest(atOrBefore: time)
            let inks = (0 ..< count).map { cell in
                CellInk.cooling(time - keys.time(head - positive(head - cell, count)))
            }
            #expect(frames.inks(at: time) == inks)
            let still = time - keys.time(head)
            let lit = still < 0.24 || Int((still - 0.24) / 0.42) % 2 == 1
            #expect(frames.cursor(at: time) == (lit ? positive(head, count) : nil))
        }
    }
}

@Test func aLineTypesFromTheLeftAndStartsAgainOnAClearLine() {
    for count in [3, 12, 40, 70] {
        let lap = count + 6
        let frames = CellTrackFrames.line(count: count)
        let keys = Keystrokes(lap: lap)
        for sample in 0 ..< 240 {
            let time = keys.period * (Double(sample) + 0.37) / 240
            let head = keys.latest(atOrBefore: time)
            let reached = positive(head, lap)
            let inks = (0 ..< count).map { cell in
                cell <= reached ? CellInk.cooling(time - keys.time(head - (reached - cell))) : .mix(1)
            }
            #expect(frames.inks(at: time) == inks)
            #expect(frames.cursor(at: time) == (reached < count ? reached : nil))
        }
    }
}

@Test func everyKeyOfAPeriodStartsAtZeroAndChangesTheValue() {
    for frames in [CellTrackFrames.rim(count: 18), .line(count: 12)] {
        for keys in frames.cells {
            #expect(keys.first?.time == 0)
            #expect(zip(keys, keys.dropFirst()).allSatisfy { $0.time < $1.time && $0.value != $1.value })
            #expect(keys.allSatisfy { $0.time < frames.period })
        }
        #expect(frames.cursor.first?.time == 0)
        #expect(zip(frames.cursor, frames.cursor.dropFirst()).allSatisfy { $0.time < $1.time && $0.value != $1.value })
    }
}

@Test func theRimStartsAtTheTopAndGoesClockwise() throws {
    let shape = try #require(CellTrackShape.rim(in: CGRect(x: 3, y: 3, width: 40, height: 40), cornerRadius: 12, thickness: 2, scale: 2))
    #expect(shape.cells.count >= 8)
    let first = shape.cells[0]
    let quarter = shape.cells[shape.cells.count / 4]
    #expect(first.midY < 6 && first.midX > 23)
    #expect(quarter.midX > 40)
    #expect(shape.cursors.allSatisfy { $0.size == shape.cursors[0].size })
}

@MainActor
private func hosted(_ frame: NSRect = NSRect(x: 10, y: 10, width: 46, height: 46)) -> (NSWindow, LayerHostView) {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 160, height: 160), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    let host = LayerHostView(frame: frame)
    window.contentView?.addSubview(host)
    return (window, host)
}

@Test @MainActor func aWorkingRimTypesInCoreAnimationAndRestsWhenMotionIsReduced() throws {
    let (window, host) = hosted()
    defer { window.close() }
    let working = CellTrackArt(kind: .rim(.working, cornerRadius: 12), lineWidth: 2, margin: 3, colors: AppTheme.dark.colors, still: false)
    host.update(setup: working, environment: EnvironmentValues()) { CellTrackArt.draw(working, $0) }
    let layers = try #require(host.content.sublayers)
    let cells = layers.dropLast()
    let cursor = try #require(layers.last)
    #expect(cells.count >= 8)
    #expect(cells.allSatisfy { $0.animation(forKey: "type") != nil })
    #expect(cursor.animation(forKey: "type") != nil)
    #expect(cursor.animation(forKey: "blink") != nil)
    #expect(host.layer?.contentsAreFlipped() == true)
    #expect(try #require(cells.first).frame.midY < 8)

    let still = CellTrackArt(kind: .rim(.working, cornerRadius: 12), lineWidth: 2, margin: 3, colors: AppTheme.dark.colors, still: true)
    host.update(setup: still, environment: EnvironmentValues()) { CellTrackArt.draw(still, $0) }
    let resting = try #require(host.content.sublayers)
    #expect(resting.count == layers.count)
    #expect(resting.allSatisfy { $0.animationKeys() == nil })
    #expect(resting.last?.frame.midY ?? 99 < 8)
}

@Test @MainActor func aProgressRimFillsWithoutTypingAndBlinksItsCursorUntilFull() throws {
    let (window, host) = hosted()
    defer { window.close() }
    for (fraction, blinks) in [(0.4, true), (1.0, false)] {
        let art = CellTrackArt(kind: .rim(.progress(fraction), cornerRadius: 12), lineWidth: 2, margin: 3,
                               colors: AppTheme.light.colors, still: false)
        host.update(setup: art, environment: EnvironmentValues()) { CellTrackArt.draw(art, $0) }
        let layers = try #require(host.content.sublayers)
        #expect(layers.allSatisfy { $0.animation(forKey: "type") == nil })
        #expect(layers.contains { $0.animation(forKey: "blink") != nil } == blinks)
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
