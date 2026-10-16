import AppKit
import KodosiBridge
import os
import Sparkle

@MainActor
@Observable
final class AppUpdater: NSObject, SPUUpdaterDelegate {
    private(set) var waiting = false
    private(set) var critical = false
    private(set) var restarting = false
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
        startingUpdater: false, updaterDelegate: self, userDriverDelegate: nil
    )

    func start() {
        guard configured else { return }
        do {
            try controller.updater.start()
            automatic = controller.updater.automaticallyDownloadsUpdates
        } catch {
            Logger.app.error("The updater did not start: \(error.localizedDescription)")
        }
    }

    func check() {
        guard configured else { return }
        controller.checkForUpdates(nil)
    }

    func restart() {
        guard let install else { return check() }
        restarting = true
        install()
    }

    func restartCancelled() {
        restarting = false
    }

    func updater(_: SPUUpdater, willInstallUpdateOnQuit item: SUAppcastItem,
                 immediateInstallationBlock: @escaping () -> Void) -> Bool
    {
        install = immediateInstallationBlock
        critical = item.isCriticalUpdate
        waiting = true
        return true
    }

    func updater(_: SPUUpdater, willInstallUpdate _: SUAppcastItem) {
        restarting = true
    }
}
