import Foundation
import Observation

enum RoomSection: Int, Hashable {
    case conversation
    case tasks
    case terminals
}

@Observable
final class RoomScreenState {
    var section: RoomSection = .conversation
    var shownSection: RoomSection = .terminals
    var finishedTasks = 0
    var showsInfo = false
    var selectedText: RoomTextSelection?
}

struct RoomTextSelection: Identifiable {
    let id: String
    let text: String
}
