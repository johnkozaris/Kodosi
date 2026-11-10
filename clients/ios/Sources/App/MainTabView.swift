import KodosiBridge
import KodosiDesign
import SwiftUI

enum MainTab: Hashable {
    case terminals, rooms, dosi, people, settings
}

struct MainTabView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var tab = MainTab.terminals
    @State private var terminals = NavigationPath()
    @State private var rooms = NavigationPath()
    @State private var dosi = NavigationPath()
    @State private var people = NavigationPath()

    var body: some View {
        TabView(selection: $tab) {
            Tab("Terminals", systemImage: "apple.terminal", value: .terminals) {
                NavigationStack(path: $terminals) { TerminalsHome() }
                    .environment(\.openRoute, OpenRouteAction { terminals.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(model.terminals.needsYou.count)
            .accessibilityIdentifier("tab.terminals")

            Tab("Projects", systemImage: "square.grid.2x2", value: .rooms) {
                NavigationStack(path: $rooms) { RoomsHome() }
                    .environment(\.openRoute, OpenRouteAction { rooms.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(model.rooms.invitations.count + model.rooms.missions.count(where: \.unread))
            .accessibilityIdentifier("tab.rooms")

            Tab("Dosi", systemImage: "eyes", value: .dosi) {
                NavigationStack(path: $dosi) { DosiHome { tab = $0 } }
                    .environment(\.openRoute, OpenRouteAction { dosi.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(dosiCalls)
            .accessibilityIdentifier("tab.dosi")

            Tab("Friends", systemImage: "person.2", value: .people) {
                NavigationStack(path: $people) { PeopleHome() }
                    .environment(\.openRoute, OpenRouteAction { people.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(model.people.incoming.count)
            .accessibilityIdentifier("tab.people")

            Tab("Settings", systemImage: "gearshape", value: .settings) {
                NavigationStack { SettingsHome() }
                    .tint(theme.colors.ink)
            }
            .badge(model.account.deviceRequests.count + (model.account.otherDeviceReset == nil ? 0 : 1))
            .accessibilityIdentifier("tab.settings")
        }
        .tabViewStyle(.sidebarAdaptable)
        .tabBarMinimizeBehavior(.onScrollDown)
        .tint(theme.colors.accentStrong)
        .onChange(of: model.terminals.sessions, initial: true) { _, sessions in model.push.remember(sessions) }
        .onChange(of: model.terminals.needsYou.count, initial: true) { _, count in model.push.showWaiting(count) }
        .onChange(of: model.terminals.watchingSessionId) { _, terminal in
            if let terminal {
                model.push.seen(terminal)
            }
        }
        .onChange(of: model.push.opened, initial: true) { _, terminal in
            guard let terminal else { return }
            model.push.opened = nil
            tab = .terminals
            terminals = NavigationPath([TerminalRoute(sessionId: terminal)])
        }
    }

    /// Dosi's open questions, or one for an answer that the person did not see.
    private var dosiCalls: Int {
        let questions = model.dosi.openQuestions.count
        return questions > 0 ? questions : model.dosi.unseen ? 1 : 0
    }
}
