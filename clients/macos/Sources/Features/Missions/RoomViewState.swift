import CoreGraphics
import Observation

@MainActor
@Observable
final class RoomViewState {
    var canvas = 0 {
        didSet { fresh.remove(canvas) }
    }

    var fresh: Set<Int> = []
    var conversationVisible = true
    var conversationWidth: CGFloat = 380
    var peopleVisible = false
    var renaming = false
    var selectedTerminal: String?
    var message = "" {
        didSet {
            if message != oldValue {
                messageRevision += 1
            }
        }
    }

    var messageRevision = 0
    var taskTitle = "" {
        didSet { taskRevision += 1 }
    }

    var taskDescription = "" {
        didSet { taskRevision += 1 }
    }

    var taskRepositories: Set<String> = [] {
        didSet { taskRevision += 1 }
    }

    var taskRevision = 0
    var repositoryURL = "" {
        didSet { repositoryRevision += 1 }
    }

    var repositoryRevision = 0
    var selectedRepository: String?
    var expandedTask: String?
    var completionNotes: [String: String] = [:]
    var taskFilter = ""
    var showsRepositoryForm = false
    var showsCompleted = false
    var followsLatest = true
    var readingMessage: String?
    var unread = 0
    var loading = false
    var busy = false
    var failure: String?
}
