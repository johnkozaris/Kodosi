import KodosiBridge
import KodosiTerminal

public extension TerminalLook {
    var palette: TerminalPalette {
        switch self {
        case .dark: .dark
        case .light: .light
        }
    }

    /// Higher contrast in light mode keeps colors chosen for dark terminals legible.
    var minimumContrast: Double {
        switch self {
        case .dark: 1.1
        case .light: 3
        }
    }
}
