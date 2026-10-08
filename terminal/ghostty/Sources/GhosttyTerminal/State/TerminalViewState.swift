import Combine

@MainActor
public final class TerminalViewState: ObservableObject {
    @Published public internal(set) var surfaceSize: TerminalGridMetrics?

    @Published public internal(set) var workingDirectory: String?

    @Published public internal(set) var lastCommandExitCode: Int?
    @Published public internal(set) var lastCommandDurationNanos: UInt64?

    public internal(set) weak var surface: TerminalSurface?

    @Published public var configuration: TerminalSurfaceOptions
    public var onClose: ((Bool) -> Void)?
    public var onSurfaceAttached: (() -> Void)?
    @Published public internal(set) var controller: TerminalController

    public init(
        controller: TerminalController,
        configuration: TerminalSurfaceOptions
    ) {
        self.controller = controller
        self.configuration = configuration
    }
}
