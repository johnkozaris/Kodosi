import AppKit
import Foundation

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

    func openQuickTerminal() {
        if let event = NSApp?.currentEvent, event.type == .keyDown, event.isARepeat {
            return
        }
        guard !typeAhead.isHolding else { return }
        workbench.launcher = nil
        guard workbench.stagedSessionIds.count < WorkbenchState.maximumStagedSessions else {
            errorMessage = String(localized: "Six terminals are open. Minimize one to open another.")
            return
        }
        typeAhead.hold()
        let directory = quickFolder
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await typeAhead.expect(createSession(name: generateSessionName(), directory: directory))
            } catch {
                typeAhead.drop()
                errorMessage = error.localizedDescription
            }
        }
    }
}
