import AppKit
import KodosiBridge
import KodosiTerminalSessions

extension TerminalSessionManager {
    convenience init(
        runtimeHandle: RuntimeHandle,
        commandSink: CommandSink,
        settings: DesktopSettings,
        disconnectTerminal: Disconnect? = nil,
        beep: @escaping @MainActor () -> Void = { NSSound.beep() },
        onFocusResult: @escaping @MainActor (String, String, String, Bool) -> Void = { _, _, _, _ in },
    ) {
        self.init(
            runtimeHandle: runtimeHandle,
            commandSink: commandSink,
            style: { KodosiTerminalStyle.make(settings: settings.terminalSettings, look: $0) },
            disconnectTerminal: disconnectTerminal,
            beep: beep,
            onFocusResult: onFocusResult,
        )
    }
}
