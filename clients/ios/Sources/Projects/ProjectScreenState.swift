import Foundation
import Observation

enum ProjectSection: Int, Hashable {
    case conversation
    case tasks
    case terminals
}

@Observable
final class ProjectScreenState {
    var section: ProjectSection = .conversation
    var shownSection: ProjectSection = .terminals
    var finishedTasks = 0
    var showsInfo = false
    var selectedText: ProjectTextSelection?
    var memberAction = MemberAction()
}

struct ProjectTextSelection: Identifiable {
    let id: String
    let text: String
}
