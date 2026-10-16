import Foundation

public enum RefreshWait {
    @MainActor
    public static func until(_ done: () -> Bool, timeout: Duration = .seconds(10)) async {
        let deadline = ContinuousClock.now + timeout
        while !done(), ContinuousClock.now < deadline, !Task.isCancelled {
            try? await Task.sleep(for: .milliseconds(100))
        }
    }
}
