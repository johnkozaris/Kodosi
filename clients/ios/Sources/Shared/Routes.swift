import SwiftUI

struct TerminalRoute: Hashable {
    let sessionId: String
}

struct RoomRoute: Hashable {
    let roomId: String
}

/// Shows a route in the navigation stack of the current tab, for a row that is a key and not a
/// navigation link.
struct OpenRouteAction {
    private let push: (any Hashable) -> Void

    init(_ push: @escaping (any Hashable) -> Void) {
        self.push = push
    }

    func callAsFunction(_ route: some Hashable) {
        push(route)
    }
}

extension EnvironmentValues {
    @Entry var openRoute = OpenRouteAction { _ in }
}
