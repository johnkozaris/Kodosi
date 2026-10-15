public struct TerminalTheme: Sendable, Hashable {
    public var light: TerminalConfiguration
    public var dark: TerminalConfiguration

    public init(
        light: TerminalConfiguration = .init(),
        dark: TerminalConfiguration = .init()
    ) {
        self.light = light
        self.dark = dark
    }

    func configuration(for colorScheme: TerminalColorScheme) -> TerminalConfiguration {
        switch colorScheme {
        case .light:
            light
        case .dark:
            dark
        }
    }
}
