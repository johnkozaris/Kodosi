/// What a viewer does with its pointer and keys on a stream that it watches: the shared browser,
/// whose coordinates are page CSS pixels, or a computer's screen, whose coordinates are the points
/// of the shown display or window.
public enum ViewerInput: Encodable, Equatable, Sendable {
    public enum MouseEvent: String, Encodable, Sendable {
        case mousePressed = "press", mouseReleased = "release", mouseMoved = "move", mouseWheel = "wheel"
    }

    public enum MouseButton: String, Encodable, Sendable { case none, left, middle, right }
    public enum KeyEvent: String, Encodable, Sendable { case keyDown = "down", keyUp = "up" }

    public struct Modifiers: OptionSet, Encodable, Sendable {
        public let rawValue: UInt8
        public init(rawValue: UInt8) {
            self.rawValue = rawValue
        }

        public static let alt = Modifiers(rawValue: 1)
        public static let control = Modifiers(rawValue: 2)
        public static let meta = Modifiers(rawValue: 4)
        public static let shift = Modifiers(rawValue: 8)
    }

    public struct Point: Encodable, Equatable, Sendable {
        public let x: Double
        public let y: Double

        public init(x: Double, y: Double) {
            self.x = x
            self.y = y
        }
    }

    case mouse(MouseEvent, x: Double, y: Double, button: MouseButton = .none, clicks: Int = 0, deltaX: Double = 0, deltaY: Double = 0,
               modifiers: Modifiers = [])
    case key(KeyEvent, key: String, code: String, text: String? = nil, keyCode: Int = 0, modifiers: Modifiers = [])
    case text(String)

    var isPlainMove: Bool {
        if case .mouse(.mouseMoved, _, _, .none, _, _, _, _) = self {
            return true
        }
        return false
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .mouse(event, x, y, button, clicks, deltaX, deltaY, modifiers):
            try container.encode("mouse", forKey: .kind)
            try container.encode(event, forKey: .action)
            try container.encode(x, forKey: .x)
            try container.encode(y, forKey: .y)
            try container.encode(button, forKey: .button)
            try container.encode(clicks, forKey: .clicks)
            try container.encode(deltaX, forKey: .deltaX)
            try container.encode(deltaY, forKey: .deltaY)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .key(event, key, code, text, keyCode, modifiers):
            try container.encode("key", forKey: .kind)
            try container.encode(event, forKey: .action)
            try container.encode(key, forKey: .key)
            try container.encode(code, forKey: .code)
            try container.encodeIfPresent(text, forKey: .text)
            try container.encode(keyCode, forKey: .keyCode)
            try container.encode(modifiers.rawValue, forKey: .modifiers)
        case let .text(text):
            try container.encode("text", forKey: .kind)
            try container.encode(text, forKey: .text)
        }
    }

    private enum Field: String, CodingKey {
        case kind, action, x, y, button, clicks, deltaX, deltaY, modifiers, key, code, text, keyCode
    }
}
