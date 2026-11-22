import KodosiBridge
import KodosiTerminal

public extension TerminalLook {
    var palette: TerminalPalette {
        switch self {
        case .dark: .dark
        case .light: .light
        }
    }

    /// A program that still draws for a dark ground stays readable on the light one.
    var minimumContrast: Double {
        switch self {
        case .dark: 1.1
        case .light: 3
        }
    }
}
