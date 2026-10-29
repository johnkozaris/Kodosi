import KodosiDesign
import SwiftUI

/// The body of a terminal and its canvas: two lobes of one surface in the terminal's color with
/// a soft waist between them, a header bar on each lobe, and the neck to the tab that it flows
/// out of. With no depth, the waist and the gap between the bars close: one lobe and one bar.
struct CanvasBody: View, Animatable {
    static let band: CGFloat = 38
    static let waist: CGFloat = 14
    static let split: CGFloat = 10
    /// The room that a header leaves free at the waist.
    static let clearance: CGFloat = 10

    @Environment(\.theme) private var theme
    var seam: CGFloat
    var depth: CGFloat
    var radius: CGFloat
    var tab: CGRect?
    var outline = true
    var shadow = true

    var animatableData: AnimatablePair<AnimatablePair<CGFloat, CGFloat>, AnimatablePair<CGFloat, CGFloat>> {
        get { AnimatablePair(AnimatablePair(seam, depth), AnimatablePair(tab?.midX ?? 0, tab?.width ?? 0)) }
        set {
            seam = newValue.first.first
            depth = newValue.first.second
            if let tab {
                self.tab = CGRect(x: newValue.second.first - newValue.second.second / 2, y: tab.minY,
                                  width: newValue.second.second, height: tab.height)
            }
        }
    }

    var body: some View {
        let lobes = Lobes(seam: seam, depth: depth, radius: radius)
        let part = min(1, depth / Self.waist)
        let gap = Self.split * part
        let end = Radius.xs * part
        let band = theme.colors.band
        ZStack(alignment: .topLeading) {
            lobes.fill(theme.colors.terminal)
            GeometryReader { geometry in
                let width = geometry.size.width
                UnevenRoundedRectangle(bottomTrailingRadius: end, style: .continuous).fill(band)
                    .frame(width: max(0, min(width, seam - gap / 2)), height: Self.band)
                UnevenRoundedRectangle(bottomLeadingRadius: end, style: .continuous).fill(band)
                    .frame(width: max(0, width - seam - gap / 2), height: Self.band)
                    .offset(x: min(width, seam + gap / 2))
            }
            .clipShape(lobes)
            if let tab {
                Neck(tab: tab, radius: radius).fill(band)
            }
        }
        .compositingGroup()
        .shadow(color: shadow ? theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.14) : .clear, radius: 10, x: 3, y: 6)
        .overlay {
            if outline {
                Outline(lobes: lobes, tab: tab).fill(theme.colors.hairline.opacity(0.7))
            }
        }
        .terminalScope()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

extension AppTheme.Colors {
    /// The header bar of a terminal's body.
    var band: Color {
        terminal.mix(with: raised, by: 0.75)
    }
}

/// The hairline around a body and the neck to its tab, which stops where the tab begins.
private struct Outline: Shape {
    let lobes: Lobes
    let tab: CGRect?

    func path(in rect: CGRect) -> Path {
        var edge = lobes.path(in: rect)
        guard let tab else { return edge.strokedPath(StrokeStyle(lineWidth: 0.5)) }
        edge = edge.union(Neck(tab: tab, radius: lobes.radius).path(in: rect))
        let above = CGRect(x: rect.minX - 1000, y: tab.maxY - 1000, width: rect.width + 2000, height: 1000.5)
        return edge.strokedPath(StrokeStyle(lineWidth: 0.5)).subtracting(Path(above))
    }
}

/// A rounded body with two lobes that a waist joins: the top edge dips between them, and the
/// bottom edge dips a little, within the padding of what the lobes hold.
struct Lobes: Shape {
    var seam: CGFloat
    var depth: CGFloat
    var radius: CGFloat

    static let reach: CGFloat = 20
    static let floor: CGFloat = 5
    static let floorReach: CGFloat = 12

    var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get { AnimatablePair(seam, depth) }
        set {
            seam = newValue.first
            depth = newValue.second
        }
    }

    func path(in rect: CGRect) -> Path {
        let r = min(radius, rect.width / 2, rect.height / 2)
        let x = rect.minX + seam
        let edge = min(x - rect.minX, rect.maxX - x)
        func dip(_ depth: CGFloat, reach: CGFloat) -> CGFloat {
            depth * max(0, min(1, (edge - reach) / (r + reach)))
        }
        let top = dip(depth, reach: Self.reach)
        let bottom = dip(depth / CanvasBody.waist * Self.floor, reach: Self.floorReach)
        var path = Path()
        path.move(to: CGPoint(x: rect.minX + r, y: rect.minY))
        if top > 0.5 {
            Self.valley(&path, at: x, y: rect.minY, depth: top, reach: Self.reach)
        }
        path.addLine(to: CGPoint(x: rect.maxX - r, y: rect.minY))
        Self.corner(&path, to: CGPoint(x: rect.maxX, y: rect.minY + r), at: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - r))
        Self.corner(&path, to: CGPoint(x: rect.maxX - r, y: rect.maxY), at: CGPoint(x: rect.maxX, y: rect.maxY))
        if bottom > 0.5 {
            Self.valley(&path, at: x, y: rect.maxY, depth: -bottom, reach: -Self.floorReach)
        }
        path.addLine(to: CGPoint(x: rect.minX + r, y: rect.maxY))
        Self.corner(&path, to: CGPoint(x: rect.minX, y: rect.maxY - r), at: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + r))
        Self.corner(&path, to: CGPoint(x: rect.minX + r, y: rect.minY), at: CGPoint(x: rect.minX, y: rect.minY))
        path.closeSubpath()
        return path
    }

    /// A soft dip of the edge at `y` around `x`, from `x - reach` to `x + reach`.
    private static func valley(_ path: inout Path, at x: CGFloat, y: CGFloat, depth: CGFloat, reach: CGFloat) {
        path.addLine(to: CGPoint(x: x - reach, y: y))
        path.addCurve(to: CGPoint(x: x, y: y + depth),
                      control1: CGPoint(x: x - reach / 2, y: y), control2: CGPoint(x: x - reach / 2, y: y + depth))
        path.addCurve(to: CGPoint(x: x + reach, y: y),
                      control1: CGPoint(x: x + reach / 2, y: y + depth), control2: CGPoint(x: x + reach / 2, y: y))
    }

    /// A soft corner from the path's current point to `end` around the corner point `at`.
    private static func corner(_ path: inout Path, to end: CGPoint, at corner: CGPoint) {
        guard let start = path.currentPoint else { return }
        let pull = 0.45
        path.addCurve(to: end,
                      control1: CGPoint(x: start.x + (corner.x - start.x) * (1 - pull), y: start.y + (corner.y - start.y) * (1 - pull)),
                      control2: CGPoint(x: end.x + (corner.x - end.x) * (1 - pull), y: end.y + (corner.y - end.y) * (1 - pull)))
    }
}

/// The neck from a tab above a body down into the body's top edge, narrow in the middle and
/// wide where it meets each of them. It meets the body only where the body's edge is
/// straight, between the rounded corners of `radius`.
struct Neck: Shape {
    var tab: CGRect
    var radius: CGFloat

    func path(in rect: CGRect) -> Path {
        let gap = -tab.maxY
        guard gap > 0, tab.width > tab.height else { return Path() }
        let middle = tab.midX
        let top = (tab.width - tab.height) / 2
        let corner = min(radius, rect.width / 2, rect.height / 2)
        let first = rect.minX + corner
        let last = rect.maxX - corner
        let narrow = min(top, max(10, tab.width * 0.28), max(4, middle - first), max(4, last - middle))
        let flare = min(10, gap + 4)
        let left = min(flare, max(0, middle - narrow - first))
        let right = min(flare, max(0, last - middle - narrow))
        let waist = tab.maxY + gap / 2
        var path = Path()
        path.move(to: CGPoint(x: middle - top, y: tab.maxY - 0.5))
        path.addLine(to: CGPoint(x: middle - top, y: tab.maxY))
        path.addCurve(to: CGPoint(x: middle - narrow, y: waist),
                      control1: CGPoint(x: middle - top + (top - narrow) * 0.6, y: tab.maxY),
                      control2: CGPoint(x: middle - narrow, y: waist - gap * 0.3))
        path.addCurve(to: CGPoint(x: middle - narrow - left, y: 0),
                      control1: CGPoint(x: middle - narrow, y: waist + gap * 0.3),
                      control2: CGPoint(x: middle - narrow - left * 0.4, y: 0))
        path.addLine(to: CGPoint(x: middle - narrow - left, y: 1))
        path.addLine(to: CGPoint(x: middle + narrow + right, y: 1))
        path.addLine(to: CGPoint(x: middle + narrow + right, y: 0))
        path.addCurve(to: CGPoint(x: middle + narrow, y: waist),
                      control1: CGPoint(x: middle + narrow + right * 0.4, y: 0),
                      control2: CGPoint(x: middle + narrow, y: waist + gap * 0.3))
        path.addCurve(to: CGPoint(x: middle + top, y: tab.maxY),
                      control1: CGPoint(x: middle + narrow, y: waist - gap * 0.3),
                      control2: CGPoint(x: middle + top - (top - narrow) * 0.6, y: tab.maxY))
        path.addLine(to: CGPoint(x: middle + top, y: tab.maxY - 0.5))
        path.closeSubpath()
        return path
    }
}
