import Foundation
import Observation

enum RoomSection: Int, Hashable {
    case conversation
    case tasks
    case terminals
}

@Observable
final class RoomViewState {
    var section: RoomSection = .conversation
    var shownSection: RoomSection = .terminals
    var message = "" {
        didSet {
            if message != oldValue {
                messageRevision += 1
            }
        }
    }

    private(set) var messageRevision = 0
    var sending = false
    var sent = 0
    var finishedTasks = 0
    var failure: String?
    var readFailure: String?
    var loading = false
    var followsLatest = true
    var watchingConversation = false
    var watchingTasks = false {
        didSet {
            if watchingTasks {
                freshTasks = false
            }
        }
    }

    var freshTasks = false
    var showsDone = false
    var showsInfo = false
    var selectedText: RoomTextSelection?
    var readingMessage: String?
    var notes: [String: String] = [:]
    var repositoryFilter: String?

    var reading: Bool {
        watchingConversation && followsLatest
    }
}

struct RoomTextSelection: Identifiable {
    let id: String
    let text: String
}
