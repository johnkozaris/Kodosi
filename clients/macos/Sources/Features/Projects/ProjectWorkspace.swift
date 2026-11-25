import CoreGraphics
import Observation

@Observable
final class ProjectWorkspace {
    var shownSection = 0
    var conversationVisible = true
    var conversationWidth: CGFloat = 380
    var peopleVisible = false
    var optionsVisible = false
    var renaming = false
    var selectedTerminal: String?
    var showsCanvas = false
    var canvasWide = false
    var canvasWidth: CGFloat?
    var taskTitle = "" {
        didSet { taskRevision += 1 }
    }

    var taskDescription = "" {
        didSet { taskRevision += 1 }
    }

    var taskRepositories: Set<String> = [] {
        didSet { taskRevision += 1 }
    }

    var taskIssueRepository: String? {
        didSet { taskRevision += 1 }
    }

    var taskRevision = 0
    var selectedRepository: String?
    var expandedTask: String?
    var showsRepositoryPicker = false
    var showsIssuePicker = false
    /// True while a task or issue request is in flight.
    var busy = false
}
