import AppKit
import KodosiBridge

extension AppDependencies {
    /// Keys reach this before the terminal, so a program never gets a key of the table.
    func startShortcuts() {
        guard shortcutMonitor == nil else { return }
        shortcutMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            self?.handleKey(event) == true ? nil : event
        }
    }

    private func handleKey(_ event: NSEvent) -> Bool {
        if let recorder = shortcutRecorder {
            return recorder(event)
        }
        guard let chord = KeyChord(event: event), chord.canBeShortcut, let (action, number) = settings.shortcuts.match(chord),
              event.window === MainWindowLocator.mainWindow(), canPerform(action),
              chord.key != "↩" || !(event.window?.firstResponder is NSText)
        else { return false }
        DispatchQueue.main.async { self.perform(action, number: number) }
        return true
    }

    func canPerform(_ action: AppShortcut) -> Bool {
        let selected = workbench.stageTakesKeys && workbench.selectedSessionId != nil
        let staged = workbench.stageTakesKeys && !workbench.stagedSessionIds.isEmpty
        return switch action {
        case .details, .zoom, .minimize: selected
        case .previousTerminal, .nextTerminal, .numberedTerminal: staged
        case .leftTerminal, .rightTerminal, .upTerminal, .downTerminal: selected
        case .close: workbench.stageTakesKeys
        case .dosi: workbench.dosiOn
        case .stopDosi: dosi.busy && workbench.dosiOpen
        default: true
        }
    }

    func perform(_ action: AppShortcut, number: Int? = nil) {
        switch action {
        case .newTerminal: openLauncher()
        case .quickTerminal: openQuickTerminal()
        case .resume: workbench.showsHistory = true
        case .close: closeSelected()
        case .goTo: workbench.showsPalette.toggle()
        case .sidebar: workbench.sidebarCollapsed.toggle()
        case .dosi: workbench.toggleDosi()
        case .stopDosi: dosi.stop()
        case .settings: workbench.section = .settings
        case .fullScreen: MainWindowLocator.mainWindow()?.toggleFullScreen(nil)
        case .minimize, .zoom, .details: performOnSelected(action)
        default: move(action, number: number)
        }
        reportVisible()
    }

    private func performOnSelected(_ action: AppShortcut) {
        guard let selected = workbench.selectedSessionId else { return }
        switch action {
        case .minimize: dismissSession(selected)
        case .zoom: workbench.toggleFocus(selected)
        default: workbench.detailsSessionId = selected
        }
    }

    private func move(_ action: AppShortcut, number: Int?) {
        switch action {
        case .previousTerminal: workbench.selectAdjacentSession(offset: -1)
        case .nextTerminal: workbench.selectAdjacentSession(offset: 1)
        case .leftTerminal: selectTerminal(.left)
        case .rightTerminal: selectTerminal(.right)
        case .upTerminal: selectTerminal(.up)
        case .downTerminal: selectTerminal(.down)
        default: selectTerminal(number: number ?? 1)
        }
    }

    private func closeSelected() {
        let main = MainWindowLocator.mainWindow()
        let window = NSApp.keyWindow ?? main
        if window === main, workbench.section == .sessions, let selected = workbench.selectedSessionId,
           workbench.stagedSessionIds.contains(selected)
        {
            workbench.closeRequest = selected
        } else {
            window?.performClose(nil)
        }
    }

    /// Terminal 9 is the last one, as in Ghostty.
    private func selectTerminal(number: Int) {
        let staged = workbench.stagedSessionIds
        guard !staged.isEmpty else { return }
        workbench.showSession(number == 9 ? staged[staged.count - 1] : staged[min(number, staged.count) - 1])
    }

    private func selectTerminal(_ direction: TerminalTilesLayout.Direction) {
        guard workbench.focusedSessionId == nil, let room = workbench.tileRoom,
              let selected = workbench.selectedSessionId,
              let index = workbench.stagedSessionIds.firstIndex(of: selected),
              let next = TerminalTilesLayout.neighbor(
                  of: index, count: workbench.stagedSessionIds.count, in: CGRect(origin: .zero, size: room), direction,
              )
        else { return }
        workbench.selectSession(workbench.stagedSessionIds[next])
    }
}
