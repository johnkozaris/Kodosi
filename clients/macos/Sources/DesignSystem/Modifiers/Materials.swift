import SwiftUI

struct ShadowSpec {
    let opacity: Double
    let radius: CGFloat
    let x: CGFloat
    let y: CGFloat

    static let none = ShadowSpec(opacity: 0, radius: 0, x: 0, y: 0)
}

enum Elevation {
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

struct RaisedBackground<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    let shape: S
    let fill: Color?
    let elevation: Elevation
    var rim = true

    var body: some View {
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

struct WellBackground<S: InsettableShape>: View {
    @Environment(\.theme) private var theme
    let shape: S
    var fill: Color?

    var body: some View {
        shape
            .fill((fill ?? theme.colors.well).shadow(.inner(
                color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.1), radius: 2, y: 1
            )))
            .overlay { shape.strokeBorder(theme.colors.hairline.opacity(theme.isDark ? 0.35 : 0.45), lineWidth: 0.5) }
    }
}

extension View {
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

struct GroundBackdrop: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ZStack {
            theme.colors.ground
            RadialGradient(
                colors: [theme.colors.accent.opacity(theme.isDark ? 0.09 : 0.07), .clear],
                center: .topLeading, startRadius: 0, endRadius: 620
            )
            Image(nsImage: GrainTexture.shared)
                .resizable(resizingMode: .tile)
                .opacity(theme.isDark ? 0.05 : 0.035)
                .blendMode(theme.isDark ? .screen : .multiply)
        }
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }
}

@MainActor
enum GrainTexture {
    static let shared: NSImage = make()

    private static func make() -> NSImage {
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
        let image = NSImage(size: NSSize(width: side / 2, height: side / 2))
        guard let provider = CGDataProvider(data: Data(pixels) as CFData),
              let cgImage = CGImage(
                  width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: side * 4,
                  space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
              )
        else { return image }
        image.addRepresentation(NSBitmapImageRep(cgImage: cgImage))
        return image
    }
}
