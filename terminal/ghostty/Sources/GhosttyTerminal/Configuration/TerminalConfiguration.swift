import Foundation

public enum TerminalCursorStyle: String, Sendable, Hashable {
    case block
    case bar
    case underline
}

public enum TerminalConfigCommand: Sendable, Hashable {
    case fontFamily(String)
    case fontSize(Float)
    case fontThicken(Bool)

    case cursorStyle(TerminalCursorStyle)
    case cursorStyleBlink(Bool)
    case cursorColor(String)

    case background(String)
    case foreground(String)
    case selectionBackground(String)
    case selectionForeground(String)
    case palette(index: Int, color: String)
    case minimumContrast(Double)

    case windowPaddingX(Int)
    case windowPaddingY(Int)

    case custom(key: String, value: String)

    var renderedLine: String {
        switch self {
        case let .fontFamily(value):
            "font-family = \(value)"

        case let .fontSize(value):
            "font-size = \(value.formatted(.number.locale(Locale(identifier: "en_US_POSIX")).grouping(.never).precision(.fractionLength(0 ... 2))))"

        case let .fontThicken(enabled):
            "font-thicken = \(enabled)"

        case let .cursorStyle(style):
            "cursor-style = \(style.rawValue)"

        case let .cursorStyleBlink(enabled):
            "cursor-style-blink = \(enabled)"

        case let .cursorColor(value):
            "cursor-color = \(value)"

        case let .background(value):
            "background = \(value)"

        case let .foreground(value):
            "foreground = \(value)"

        case let .selectionBackground(value):
            "selection-background = \(value)"

        case let .selectionForeground(value):
            "selection-foreground = \(value)"

        case let .palette(index, color):
            "palette = \(index)=\(color)"

        case let .minimumContrast(value):
            "minimum-contrast = \(value.formatted(.number.locale(Locale(identifier: "en_US_POSIX")).grouping(.never).precision(.fractionLength(0 ... 2))))"

        case let .windowPaddingX(value):
            "window-padding-x = \(value)"

        case let .windowPaddingY(value):
            "window-padding-y = \(value)"

        case let .custom(key, value):
            "\(key) = \(value)"
        }
    }
}

public struct TerminalConfiguration: Sendable, Hashable {
    public struct Builder {
        var commands: [TerminalConfigCommand] = []

        public init() {}

        init(commands: [TerminalConfigCommand]) {
            self.commands = commands
        }

        public mutating func withFontFamily(_ value: String) {
            commands.append(.fontFamily(value))
        }

        public mutating func withFontSize(_ value: Float) {
            commands.append(.fontSize(value))
        }

        public mutating func withFontThicken(_ enabled: Bool) {
            commands.append(.fontThicken(enabled))
        }

        public mutating func withCursorStyle(_ style: TerminalCursorStyle) {
            commands.append(.cursorStyle(style))
        }

        public mutating func withCursorStyleBlink(_ enabled: Bool) {
            commands.append(.cursorStyleBlink(enabled))
        }

        public mutating func withCursorColor(_ value: String) {
            commands.append(.cursorColor(value))
        }

        public mutating func withBackground(_ value: String) {
            commands.append(.background(value))
        }

        public mutating func withForeground(_ value: String) {
            commands.append(.foreground(value))
        }

        public mutating func withSelectionBackground(_ value: String) {
            commands.append(.selectionBackground(value))
        }

        public mutating func withSelectionForeground(_ value: String) {
            commands.append(.selectionForeground(value))
        }

        public mutating func withPalette(_ index: Int, color: String) {
            commands.append(.palette(index: index, color: color))
        }

        public mutating func withMinimumContrast(_ value: Double) {
            commands.append(.minimumContrast(value))
        }

        public mutating func withWindowPaddingX(_ value: Int) {
            commands.append(.windowPaddingX(value))
        }

        public mutating func withWindowPaddingY(_ value: Int) {
            commands.append(.windowPaddingY(value))
        }

        public mutating func withCustom(_ key: String, _ value: String) {
            commands.append(.custom(key: key, value: value))
        }
    }

    let commands: [TerminalConfigCommand]

    public init() {
        commands = []
    }

    public init(configure: (inout Builder) -> Void) {
        self.init(startingFrom: .init(), configure: configure)
    }

    public init(
        startingFrom base: TerminalConfiguration,
        configure: (inout Builder) -> Void
    ) {
        var builder = Builder(commands: base.commands)
        configure(&builder)
        commands = builder.commands
    }

    public static let `default` = TerminalConfiguration { builder in
        builder.withCursorStyle(.block)
        builder.withCursorStyleBlink(true)
        builder.withFontSize(14)
        builder.withFontThicken(true)
    }

    public var rendered: String {
        commands.map(\.renderedLine).joined(separator: "\n")
    }

    var isEmpty: Bool {
        commands.isEmpty
    }
}
