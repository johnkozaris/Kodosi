import Foundation
import Observation

public enum ProjectPart: Hashable, Sendable {
    case conversation
    case tasks
    case repositories
}

@MainActor
@Observable
public final class ProjectViewState {
    public var message = "" {
        didSet {
            if message != oldValue {
                messageRevision += 1
            }
        }
    }

    public private(set) var messageRevision = 0
    public internal(set) var sending = false
    /// The messages that the person posted here.
    public internal(set) var sent = 0
    public var failure: String?
    public internal(set) var loading = false
    public var readFailure: String?
    public private(set) var shown: Set<ProjectPart> = [] {
        didSet {
            fresh.subtract(shown)
            if shown.contains(.conversation), !oldValue.contains(.conversation), followsLatest {
                startedReading()
            }
        }
    }

    public var followsLatest = true {
        didSet {
            if followsLatest, !oldValue, shown.contains(.conversation) {
                startedReading()
            }
        }
    }

    public var readingMessage: String?
    /// The messages of others that arrived while the person did not read the latest.
    public internal(set) var unread = 0
    /// The parts that changed while they were not on screen.
    public internal(set) var fresh: Set<ProjectPart> = []
    public var notes: [String: String] = [:]
    public var showsDone = false
    public var repositoryFilter: String?

    @ObservationIgnored var startedReading: () -> Void = {}
    /// The views that show each part, as one view can come before another goes.
    @ObservationIgnored private var showing: [ProjectPart: Int] = [:]

    init() {}

    public func show(_ part: ProjectPart) {
        showing[part, default: 0] += 1
        shown.insert(part)
    }

    public func hide(_ part: ProjectPart) {
        let left = (showing[part] ?? 1) - 1
        showing[part] = left > 0 ? left : nil
        if left <= 0 {
            shown.remove(part)
        }
    }

    public var reading: Bool {
        shown.contains(.conversation) && followsLatest
    }

    public var canPost: Bool {
        !sending && !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}
