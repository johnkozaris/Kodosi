extension TerminalViewState: TerminalSurfaceGridResizeDelegate, TerminalSurfaceLifecycleDelegate {
    public func terminalDidResize(_ size: TerminalGridMetrics) {
        surfaceSize = size
    }

    public func terminalDidAttachSurface(_: TerminalSurface) {
        onSurfaceAttached?()
    }

    public func terminalDidDetachSurface() {}
}
