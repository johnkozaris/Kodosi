import KodosiBridge
import KodosiDesign
import SwiftUI

/// The body takes the terminal's look, while its shadow falls on the ground around it and takes
/// the surrounding theme.
struct CanvasBody: View, Animatable {
    static let band: CGFloat = 38
    nonisolated static let waist: CGFloat = 14
    static let split: CGFloat = 10
    /// Horizontal padding headers leave so content stays clear of the pinched seam.
    static let clearance: CGFloat = 10

    @Environment(\.theme) private var theme
    let look: TerminalLook
    var seam: CGFloat
    var depth: CGFloat
    var radius: CGFloat
    var tab: CGRect?
    var chatEnd: CGFloat?
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
        let colors = AppTheme(look: look).colors
        let band = colors.band
        ZStack(alignment: .topLeading) {
            lobes.fill(colors.terminal)
            if let chatEnd {
                GeometryReader { geometry in
                    colors.chat.frame(width: max(0, min(geometry.size.width, chatEnd)))
                }
                .clipShape(lobes)
            }
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
                Outline(lobes: lobes, tab: tab).fill(colors.hairline.opacity(0.7))
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

private nonisolated struct Outline: Shape {
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

@Animatable
nonisolated struct Lobes: Shape {
    var seam: CGFloat
    var depth: CGFloat
    @AnimatableIgnored var radius: CGFloat

    static let reach: CGFloat = 20
    static let floor: CGFloat = 5
    static let floorReach: CGFloat = 12

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

    private static func valley(_ path: inout Path, at x: CGFloat, y: CGFloat, depth: CGFloat, reach: CGFloat) {
        path.addLine(to: CGPoint(x: x - reach, y: y))
        path.addCurve(to: CGPoint(x: x, y: y + depth),
                      control1: CGPoint(x: x - reach / 2, y: y), control2: CGPoint(x: x - reach / 2, y: y + depth))
        path.addCurve(to: CGPoint(x: x + reach, y: y),
                      control1: CGPoint(x: x + reach / 2, y: y + depth), control2: CGPoint(x: x + reach / 2, y: y))
    }

    private static func corner(_ path: inout Path, to end: CGPoint, at corner: CGPoint) {
        guard let start = path.currentPoint else { return }
        let pull = 0.45
        path.addCurve(to: end,
                      control1: CGPoint(x: start.x + (corner.x - start.x) * (1 - pull), y: start.y + (corner.y - start.y) * (1 - pull)),
                      control2: CGPoint(x: end.x + (corner.x - end.x) * (1 - pull), y: end.y + (corner.y - end.y) * (1 - pull)))
    }
}

/// Clamps the tab neck to the straight top segment between corner radii so it joins seamlessly.
nonisolated struct Neck: Shape {
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
