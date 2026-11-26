import AppKit
import KodosiBridge
import os
import Sparkle

@Observable
final class AppUpdater: NSObject, SPUUpdaterDelegate {
    private(set) var started = false
    private(set) var waiting = false
    private(set) var critical = false
    private(set) var restarting = false
    /// Kodosi restarts by itself to install the waiting update while nobody uses it.
    private(set) var restartingQuietly = false
    /// A restart now loses nothing, and nobody uses Kodosi.
    var quiet = false {
        didSet { restartIfQuiet() }
    }

    /// Kodosi downloads updates in the background and installs them by itself while nobody uses it.
    var automatic = false {
        didSet {
            if configured {
                controller.updater.automaticallyDownloadsUpdates = automatic
            }
        }
    }

    let configured: Bool = {
        let info = Bundle.main.infoDictionary ?? [:]
        return (info["SUFeedURL"] as? String)?.hasPrefix("https://") == true
            && (info["SUPublicEDKey"] as? String)?.isEmpty == false
    }()

    @ObservationIgnored private var install: (() -> Void)?
    @ObservationIgnored private lazy var controller = SPUStandardUpdaterController(
        startingUpdater: false, updaterDelegate: self, userDriverDelegate: nil,
    )

    func start() {
        guard configured else { return }
        do {
            try controller.updater.start()
            started = true
            automatic = controller.updater.automaticallyDownloadsUpdates
        } catch {
            Logger.app.error("The updater did not start: \(error.localizedDescription)")
        }
    }

    func check() {
        guard started else { return }
        controller.checkForUpdates(nil)
    }

    func checkInBackground() {
        guard started, !waiting else { return }
        controller.updater.checkForUpdatesInBackground()
    }

    var actionTitle: String {
        waiting ? String(localized: "Restart to Update") : String(localized: "Check for Updates")
    }

    func restart() {
        guard let install else { return check() }
        restarting = true
        install()
    }

    func restartCancelled() {
        restarting = false
        restartingQuietly = false
    }

    private func restartIfQuiet() {
        guard automatic, quiet, waiting, !restarting, install != nil else { return }
        restartingQuietly = true
        restart()
    }

    func updater(_: SPUUpdater, willInstallUpdateOnQuit item: SUAppcastItem,
                 immediateInstallationBlock: @escaping () -> Void) -> Bool
    {
        install = immediateInstallationBlock
        critical = item.isCriticalUpdate
        waiting = true
        restartIfQuiet()
        return true
    }

    func updater(_: SPUUpdater, willInstallUpdate _: SUAppcastItem) {
        restarting = true
    }
}
