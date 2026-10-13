import SwiftUI

public struct ShadowSpec: Sendable {
    public let opacity: Double
    public let radius: CGFloat
    public let x: CGFloat
    public let y: CGFloat

    public init(opacity: Double, radius: CGFloat, x: CGFloat, y: CGFloat) {
        self.opacity = opacity
        self.radius = radius
        self.x = x
        self.y = y
    }

    public static let none = ShadowSpec(opacity: 0, radius: 0, x: 0, y: 0)
}

public enum Elevation: Sendable {
    case flat
    case resting
    case lifted
    case floating

    fileprivate func contact(dark: Bool) -> ShadowSpec {
        switch self {
        case .flat: .none
        case .resting: ShadowSpec(opacity: dark ? 0.42 : 0.13, radius: 3, x: 1, y: 2)
        case .lifted: ShadowSpec(opacity: dark ? 0.5 : 0.18, radius: 8, x: 2, y: 5)
        case .floating: ShadowSpec(opacity: dark ? 0.55 : 0.22, radius: 12, x: 3, y: 8)
        }
    }

    fileprivate func ambient(dark: Bool) -> ShadowSpec {
        switch self {
        case .flat: .none
        case .resting: ShadowSpec(opacity: dark ? 0.3 : 0.08, radius: 12, x: 4, y: 8)
        case .lifted: ShadowSpec(opacity: dark ? 0.42 : 0.14, radius: 26, x: 8, y: 16)
        case .floating: ShadowSpec(opacity: dark ? 0.5 : 0.18, radius: 44, x: 12, y: 26)
        }
    }
}

public struct RaisedBackground<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    public let shape: S
    public let fill: Color?
    public let elevation: Elevation
    public var rim = true

    public init(shape: S, fill: Color?, elevation: Elevation, rim: Bool = true) {
        self.shape = shape
        self.fill = fill
        self.elevation = elevation
        self.rim = rim
    }

    public var body: some View {
        let contact = elevation.contact(dark: theme.isDark)
        let ambient = elevation.ambient(dark: theme.isDark)
        shape
            .fill(fill ?? theme.colors.raised)
            .shadow(color: theme.colors.shadow.opacity(contact.opacity), radius: contact.radius, x: contact.x, y: contact.y)
            .shadow(color: theme.colors.shadow.opacity(ambient.opacity), radius: ambient.radius, x: ambient.x, y: ambient.y)
            .overlay {
                if rim {
                    shape.strokeBorder(
                        LinearGradient(
                            stops: [
                                .init(color: theme.colors.highlight, location: 0),
                                .init(color: theme.colors.highlight.opacity(0), location: 0.45),
                                .init(color: theme.colors.lowlight.opacity(0), location: 0.6),
                                .init(color: theme.colors.lowlight, location: 1),
                            ],
                            startPoint: .topLeading, endPoint: .bottomTrailing
                        ),
                        lineWidth: 1
                    )
                }
            }
            .overlay { shape.strokeBorder(theme.colors.hairline.opacity(theme.isDark ? 0.5 : 0.7), lineWidth: 0.5) }
    }
}

public enum Bezel: Sendable {
    case solid
    case quiet
}

public struct BezelBackground<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    @Environment(\.bezelPressed) private var pressed
    public let shape: S
    public let fill: Color
    public let bezel: Bezel

    public init(shape: S, fill: Color, bezel: Bezel = .solid) {
        self.shape = shape
        self.fill = fill
        self.bezel = bezel
    }

    public var body: some View {
        let quiet = bezel == .quiet
        let dark = theme.isDark
        let shadow = theme.colors.shadow
        shape
            .fill(fill.mix(with: .black, by: pressed ? 0.08 : 0))
            .overlay {
                if !dark {
                    shape.inset(by: quiet ? -0.5 : 0)
                        .strokeBorder(quiet ? shadow.opacity(0.22) : .black.opacity(0.18), lineWidth: 0.5)
                }
            }
            .overlay {
                OffsetEdge(shape: shape, offset: -1)
                    .fill(.black.opacity(dark ? 0.2 : 0), style: FillStyle(eoFill: true))
                    .clipShape(shape)
            }
            .overlay {
                OffsetEdge(shape: shape, offset: 1)
                    .fill(.white.opacity(light), style: FillStyle(eoFill: true))
                    .clipShape(shape)
            }
            .shadow(color: shadow.opacity(dark ? 0 : quiet ? 0.14 : 0.3), radius: 0.75, y: 1)
    }

    private var light: Double {
        if theme.isDark {
            return bezel == .quiet ? 0.06 : 0.05
        }
        return bezel == .quiet ? 0.35 : 0.22
    }
}

private struct OffsetEdge<S: Shape>: Shape {
    let shape: S
    let offset: CGFloat

    func path(in rect: CGRect) -> Path {
        var path = shape.path(in: rect)
        path.addPath(shape.path(in: rect).offsetBy(dx: 0, dy: offset))
        return path
    }
}

public struct WellBackground<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    public let shape: S
    public var fill: Color?

    public init(shape: S, fill: Color? = nil) {
        self.shape = shape
        self.fill = fill
    }

    public var body: some View {
        shape
            .fill((fill ?? theme.colors.well).shadow(.inner(
                color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.1), radius: 2, y: 1
            )))
            .overlay { shape.strokeBorder(theme.colors.hairline.opacity(theme.isDark ? 0.35 : 0.45), lineWidth: 0.5) }
    }
}

public extension View {
    func raised(
        _ radius: CGFloat = Radius.lg, fill: Color? = nil, elevation: Elevation = .resting
    ) -> some View {
        background(RaisedBackground(
            shape: RoundedRectangle(cornerRadius: radius, style: .continuous), fill: fill, elevation: elevation
        ))
    }

    func raisedCapsule(fill: Color? = nil, elevation: Elevation = .resting) -> some View {
        background(RaisedBackground(shape: Capsule(), fill: fill, elevation: elevation))
    }

    func well(_ radius: CGFloat = Radius.lg, fill: Color? = nil) -> some View {
        background(WellBackground(shape: RoundedRectangle(cornerRadius: radius, style: .continuous), fill: fill))
    }

    func wellCapsule(fill: Color? = nil) -> some View {
        background(WellBackground(shape: Capsule(), fill: fill))
    }

    func terminalScope() -> some View {
        modifier(TerminalScope())
    }

    func chromeScope() -> some View {
        modifier(ChromeScope())
    }

    func popoverSheet(width: CGFloat? = nil) -> some View {
        modifier(PopoverSheet(width: width))
    }

    func hairline(_ edge: Edge) -> some View {
        modifier(Hairline(edge: edge))
    }
}

private struct TerminalScope: ViewModifier {
    @Environment(\.theme) private var theme
    @Environment(\.chromeTheme) private var chrome

    func body(content: Content) -> some View {
        content
            .environment(\.chromeTheme, chrome ?? theme)
            .environment(\.theme, theme.isDark ? theme : AppTheme.dark.reducingMotion(theme.motion.reduced))
            .environment(\.colorScheme, .dark)
    }
}

private struct ChromeScope: ViewModifier {
    @Environment(\.theme) private var theme
    @Environment(\.chromeTheme) private var chrome

    func body(content: Content) -> some View {
        let outer = chrome ?? theme
        content
            .environment(\.theme, outer)
            .environment(\.colorScheme, outer.isDark ? .dark : .light)
    }
}

private struct PopoverSheet: ViewModifier {
    @Environment(\.theme) private var theme
    let width: CGFloat?

    func body(content: Content) -> some View {
        content
            .frame(width: width)
            .background(theme.colors.raised)
            .presentationBackground(theme.colors.raised)
            .environment(\.theme, theme)
            .buttonStyle(.kodosi(.secondary))
            .textFieldStyle(WellTextFieldStyle())
            .toggleStyle(SwitchToggleStyle())
    }
}

private struct Hairline: ViewModifier {
    @Environment(\.theme) private var theme
    let edge: Edge

    func body(content: Content) -> some View {
        content.overlay(alignment: alignment) {
            Rectangle().fill(theme.colors.hairline.opacity(0.6))
                .frame(width: edge == .leading || edge == .trailing ? 0.5 : nil,
                       height: edge == .top || edge == .bottom ? 0.5 : nil)
        }
    }

    private var alignment: Alignment {
        switch edge {
        case .top: .top
        case .bottom: .bottom
        case .leading: .leading
        case .trailing: .trailing
        }
    }
}

public struct GroundBackdrop: View {
    @Environment(\.theme) private var theme

    public init() {}

    public var body: some View {
        ZStack {
            theme.colors.ground
            RadialGradient(
                colors: [theme.colors.accent.opacity(theme.isDark ? 0.09 : 0.07), .clear],
                center: .topLeading, startRadius: 0, endRadius: 620
            )
            if let grain = GrainTexture.shared {
                Image(decorative: grain, scale: 2)
                    .resizable(resizingMode: .tile)
                    .opacity(theme.isDark ? 0.05 : 0.035)
                    .blendMode(theme.isDark ? .screen : .multiply)
            }
        }
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }
}

@MainActor
public enum GrainTexture {
    public static let shared: CGImage? = make()

    private static func make() -> CGImage? {
        let side = 128
        var state: UInt64 = 0x9E37_79B9_7F4A_7C15
        var pixels = [UInt8](repeating: 0, count: side * side * 4)
        for index in 0 ..< side * side {
            state = state &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
            let value = UInt8(truncatingIfNeeded: state >> 56)
            pixels[index * 4] = value
            pixels[index * 4 + 1] = value
            pixels[index * 4 + 2] = value
            pixels[index * 4 + 3] = 255
        }
        guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(
            width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: side * 4,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
        )
    }
}
