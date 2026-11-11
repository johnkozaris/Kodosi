import Foundation
import Observation

/// The newest question wins.
@MainActor
@Observable
public final class FolderBrowser {
    public let launcher: LauncherState
    public private(set) var listing: FolderList?
    public private(set) var failure: String?
    @ObservationIgnored private var asking: UUID?

    public init(launcher: LauncherState) {
        self.launcher = launcher
    }

    public func loadPlaces() async {
        do {
            _ = try await launcher.folders()
        } catch is CancellationError {
            return
        } catch {
            if launcher.places == nil {
                failure = error.localizedDescription
            }
        }
    }

    public func open(_ path: String?) async {
        let asked = UUID()
        asking = asked
        do {
            let next = try await launcher.folders(at: path)
            guard asking == asked else { return }
            failure = nil
            listing = next
        } catch is CancellationError {
            return
        } catch {
            guard asking == asked else { return }
            failure = error.localizedDescription
        }
    }
}
