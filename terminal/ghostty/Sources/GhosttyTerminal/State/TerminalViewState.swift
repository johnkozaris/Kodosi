import Combine

@MainActor
public final class TerminalViewState: ObservableObject {
    @Published public internal(set) var surfaceSize: TerminalGridMetrics?
    @Published public var configuration: TerminalSurfaceOptions
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
