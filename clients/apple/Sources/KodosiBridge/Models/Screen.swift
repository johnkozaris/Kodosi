#if os(macOS)
    import AppKit
#endif
import CoreGraphics
import Foundation
import ImageIO
import KodosiKit

/// What the computer's screen can show: its displays, the main one first, and its windows.
public struct ScreenSources: Decodable, Equatable, Sendable {
    public struct Display: Decodable, Equatable, Identifiable, Sendable {
        public let id: UInt32
        public let width: Double
        public let height: Double
        public let scale: Double
    }

    public struct Window: Decodable, Equatable, Identifiable, Sendable {
        public let id: UInt32
        public let app: String
        public let title: String
        public let width: Double
        public let height: Double
        public let scale: Double
    }

    public var displays: [Display] = []
    public var windows: [Window] = []

    public init() {}
}

/// A display or a window of the computer.
public enum ScreenSource: Codable, Hashable, Sendable {
    case display(UInt32)
    case window(UInt32)

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: Field.self)
        let id = try container.decode(UInt32.self, forKey: .id)
        switch try container.decode(String.self, forKey: .kind) {
        case "window": self = .window(id)
        default: self = .display(id)
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .display(id):
            try container.encode("display", forKey: .kind)
            try container.encode(id, forKey: .id)
        case let .window(id):
            try container.encode("window", forKey: .kind)
            try container.encode(id, forKey: .id)
        }
    }

    private enum Field: String, CodingKey { case kind, id }
}

/// What macOS lets Kodosi Screen do on the computer: record its screen, and control it with
/// Accessibility.
public struct ScreenAccess: Decodable, Equatable, Sendable {
    public let screen: Bool
    public let input: Bool

    public init(screen: Bool, input: Bool) {
        self.screen = screen
        self.input = input
    }
}

/// Why nobody at the computer sees its screen for now. Its capture stops meanwhile and starts again
/// by itself.
public enum ScreenAway: String, Decodable, Sendable {
    case locked
    case asleep
    case anotherAccount
}

/// Where the computer's pointer is, in the shown source's points, and the shape it has.
public struct ScreenPointer: Decodable, Equatable, Sendable {
    public let x: Double
    public let y: Double
    public let shape: UInt64
}

/// A pointer's picture, with its hotspot and size in points.
public struct ScreenShape: @unchecked Sendable {
    public let image: CGImage
    public let hotspot: CGPoint
    public let size: CGSize
}

/// The part of a source that a view shows, aspect-fit and centred in the view, in the source's
/// points.
public struct VideoPlacement: Equatable, Sendable {
    public let source: CGSize
    public let look: CGRect

    public init(source: CGSize, look: CGRect) {
        self.source = source
        self.look = look
    }

    /// View points per source point.
    public func fit(in view: CGSize) -> CGFloat {
        guard look.width > 0, look.height > 0 else { return 0 }
        return min(view.width / look.width, view.height / look.height)
    }

    public func viewPoint(_ point: CGPoint, in view: CGSize) -> CGPoint {
        let fit = fit(in: view)
        return CGPoint(x: (view.width - look.width * fit) / 2 + (point.x - look.minX) * fit,
                       y: (view.height - look.height * fit) / 2 + (point.y - look.minY) * fit)
    }

    public func viewRect(_ rect: CGRect, in view: CGSize) -> CGRect {
        let fit = fit(in: view)
        return CGRect(origin: viewPoint(rect.origin, in: view), size: CGSize(width: rect.width * fit, height: rect.height * fit))
    }

    /// The source point under a view point, or `nil` outside the source unless `clamped`.
    public func sourcePoint(_ point: CGPoint, in view: CGSize, clamped: Bool = false) -> CGPoint? {
        let fit = fit(in: view)
        guard fit > 0, source.width > 0, source.height > 0 else { return nil }
        var x = look.minX + (point.x - (view.width - look.width * fit) / 2) / fit
        var y = look.minY + (point.y - (view.height - look.height * fit) / 2) / fit
        if clamped {
            x = min(max(x, 0), source.width.nextDown)
            y = min(max(y, 0), source.height.nextDown)
        }
        guard x >= 0, y >= 0, x < source.width, y < source.height else { return nil }
        return CGPoint(x: x, y: y)
    }
}

/// What the screen tells a viewer on its pipe.
struct ScreenMessage: Decodable, Sendable {
    struct Shape: Decodable, Sendable {
        let id: UInt64
        let png: String
        let x: Double
        let y: Double
        let width: Double
        let height: Double

        var picture: ScreenShape? {
            guard let data = Data(base64Encoded: png), let source = CGImageSourceCreateWithData(data as CFData, nil),
                  let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
            return ScreenShape(image: image, hotspot: CGPoint(x: x, y: y), size: CGSize(width: width, height: height))
        }
    }

    /// Sent only when they changed.
    let sources: ScreenSources?
    let source: ScreenSource?
    /// The shown source's size in points.
    let width: Double
    let height: Double
    let pointer: ScreenPointer?
    /// Pointer pictures that this viewer has not had yet.
    let shapes: [Shape]
    let access: ScreenAccess
    let away: ScreenAway?
    /// True while the capture's size follows this viewer's view.
    let control: Bool
    /// Text copied on the computer, which only the viewer in control gets, once.
    let copied: String?

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        sources = try container.decodeIfPresent(ScreenSources.self, forKey: .sources)
        source = try container.decodeIfPresent(ScreenSource.self, forKey: .source)
        width = try container.decode(Double.self, forKey: .width)
        height = try container.decode(Double.self, forKey: .height)
        pointer = try container.decodeIfPresent(ScreenPointer.self, forKey: .pointer)
        shapes = try container.decodeIfPresent([Shape].self, forKey: .shapes) ?? []
        access = try container.decode(ScreenAccess.self, forKey: .access)
        // A kind of away that this app does not know yet shows the screen as it can.
        away = try? container.decodeIfPresent(ScreenAway.self, forKey: .away)
        control = try container.decode(Bool.self, forKey: .control)
        copied = try container.decodeIfPresent(String.self, forKey: .copied)
    }

    private enum CodingKeys: String, CodingKey { case sources, source, width, height, pointer, shapes, access, away, control, copied }
}

/// Whether this computer lets its owner's other devices see and control its screen, which only
/// the person at it decides, in its own app.
public struct ScreenSharing: Equatable, Sendable {
    public let allowed: Bool
    /// False where Kodosi Screen did not come with the app.
    public let available: Bool
    /// What macOS lets Kodosi Screen do, known while the sharing is on.
    public let access: ScreenAccess?

    init(_ event: RuntimeEvent) throws {
        allowed = try event.value("allowed")
        available = try event.value("available")
        access = try? event.value("access", as: ScreenAccess.self)
    }
}

public extension CommandSink {
    func screenSharing() async throws -> ScreenSharing {
        try await ScreenSharing(request("screen.status"))
    }

    /// Turning the sharing on has Kodosi Screen ask the person at this Mac whether the devices of
    /// this account may see and control it, in an alert that comes to the front because this app
    /// lets Kodosi Screen take its place there. Then it asks macOS for what it may not do yet.
    /// Only the person's click on the switch calls this with `true`: the app vouches for that
    /// click, and Kodosi Screen shows its question to nobody else.
    func shareScreen(_ allowed: Bool) async throws -> ScreenSharing {
        var fields: [String: JSONValue] = ["allowed": .bool(allowed)]
        #if os(macOS)
            if allowed {
                NSApp.yieldActivation(toApplicationWithBundleIdentifier: "com.kodosi.screen")
                if let vouch = Self.vouchForClick() {
                    fields["vouch"] = .string(vouch)
                }
            }
        #endif
        return try await ScreenSharing(request("screen.allow", fields))
    }

    #if os(macOS)
        private static func vouchForClick() -> String? {
            var bytes = [UInt8](repeating: 0, count: Int(KODOSI_SCREEN_VOUCH_BYTES))
            let length = kodosi_screen_vouch(&bytes, UInt(bytes.count))
            guard length > 0, Int(length) <= bytes.count else { return nil }
            return String(decoding: bytes.prefix(Int(length)), as: UTF8.self)
        }
    #endif
}
