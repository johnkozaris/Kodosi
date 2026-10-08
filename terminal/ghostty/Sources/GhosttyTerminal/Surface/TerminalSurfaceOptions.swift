public struct TerminalSurfaceOptions: Sendable {
    public var session: InMemoryTerminalSession
    public var fontSize: Float?
    public var terminalConfiguration: TerminalConfiguration?
    public var scrollbackLimitBytes: Int?

    public init(
        session: InMemoryTerminalSession,
        fontSize: Float? = nil,
        terminalConfiguration: TerminalConfiguration? = nil,
        scrollbackLimitBytes: Int? = nil
    ) {
        self.session = session
        self.fontSize = fontSize
        self.terminalConfiguration = terminalConfiguration
        self.scrollbackLimitBytes = scrollbackLimitBytes
    }

    func isEquivalent(to other: TerminalSurfaceOptions) -> Bool {
        requiresSurfaceRebuild(comparedTo: other) == false
            && fontSize == other.fontSize
            && terminalConfiguration == other.terminalConfiguration
            && scrollbackLimitBytes == other.scrollbackLimitBytes
    }

    func requiresSurfaceRebuild(comparedTo other: TerminalSurfaceOptions) -> Bool {
        session !== other.session
    }

    var hasSurfaceConfiguration: Bool {
        fontSize != nil
            || terminalConfiguration != nil
            || scrollbackLimitBytes != nil
    }
}
