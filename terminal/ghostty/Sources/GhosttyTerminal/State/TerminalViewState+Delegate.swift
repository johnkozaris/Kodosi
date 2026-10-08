extension TerminalViewState:
    TerminalSurfaceGridResizeDelegate,
    TerminalSurfaceCloseDelegate,
    TerminalSurfacePwdDelegate,
    TerminalSurfaceCommandFinishedDelegate,
    TerminalSurfaceLifecycleDelegate
{
    public func terminalDidResize(_ size: TerminalGridMetrics) {
        surfaceSize = size
    }

    public func terminalDidClose(processAlive: Bool) {
        onClose?(processAlive)
    }

    public func terminalDidChangeWorkingDirectory(_ path: String) {
        workingDirectory = path
    }

    public func terminalDidFinishCommand(exitCode: Int?, durationNanos: UInt64) {
        lastCommandExitCode = exitCode
        lastCommandDurationNanos = durationNanos
    }

    public func terminalDidAttachSurface(_ surface: TerminalSurface) {
        self.surface = surface
        onSurfaceAttached?()
    }

    public func terminalDidDetachSurface() {
        surface = nil
    }
}
