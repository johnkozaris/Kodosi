import AppKit
import Foundation
import KodosiBridge

extension AppDependencies {
    /// The selected terminal's folder on this computer, or else the folder that the launcher offers.
    var quickFolder: String? {
        [selectedLocalDirectory, settings.lastWorkingDir, storageBootstrap.defaultWorkingDirectory?.path]
            .compactMap(\.self)
            .first { folder in
                var isFolder: ObjCBool = false
                return FileManager.default.fileExists(atPath: folder, isDirectory: &isFolder) && isFolder.boolValue
            }
    }

    /// The agent in front of the selected terminal of this computer, to start beside it.
    var selectedAgent: AgentStart? {
        workbench.selectedSessionId.flatMap(session).flatMap { AgentStart(runningIn: $0, info: launcher.agent($0.program)) }
    }

    /// A terminal that opens at once: plain, or with `agent`.
    func openQuickTerminal(agent: AgentStart? = nil) {
        if let event = NSApp?.currentEvent, event.type == .keyDown, event.isARepeat {
            return
        }
        guard !typeAhead.isHolding else { return }
        workbench.launcher = nil
        guard workbench.stagedSessionIds.count < WorkbenchState.maximumStagedSessions || workbench.focusedSessionId != nil else {
            errorMessage = String(localized: "Six terminals are open. Minimize one to open another.")
            return
        }
        typeAhead.hold()
        let directory = quickFolder
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await typeAhead.expect(createSession(name: generateSessionName(), directory: directory, agent: agent))
            } catch {
                typeAhead.drop()
                errorMessage = error.localizedDescription
            }
        }
    }

    func stageHasRoom(for id: String) -> Bool {
        workbench.stagedSessionIds.contains(id) || workbench.stagedSessionIds.count < WorkbenchState.maximumStagedSessions
            || workbench.makingRoom(for: id) != nil
    }

    func makeRoom(for id: String) {
        if let leaving = workbench.makingRoom(for: id) {
            dismissSession(leaving)
        }
    }
}
