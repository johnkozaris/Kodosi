import SwiftUI

struct TerminalRoute: Hashable {
    let sessionId: String
}

struct ProjectRoute: Hashable {
    let projectId: String
}

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
