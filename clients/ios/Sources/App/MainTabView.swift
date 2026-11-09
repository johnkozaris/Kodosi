import KodosiBridge
import KodosiDesign
import SwiftUI

struct MainTabView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var terminals = NavigationPath()
    @State private var rooms = NavigationPath()
    @State private var people = NavigationPath()

    var body: some View {
        TabView {
            Tab("Terminals", systemImage: "apple.terminal") {
                NavigationStack(path: $terminals) { TerminalsHome() }
                    .environment(\.openRoute, OpenRouteAction { terminals.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(model.terminals.needsYou.count)
            .accessibilityIdentifier("tab.terminals")

            Tab("Rooms", systemImage: "square.grid.2x2") {
                NavigationStack(path: $rooms) { RoomsHome() }
                    .environment(\.openRoute, OpenRouteAction { rooms.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(model.rooms.invitations.count + model.rooms.missions.count(where: \.unread))
            .accessibilityIdentifier("tab.rooms")

            Tab("Friends", systemImage: "person.2") {
                NavigationStack(path: $people) { PeopleHome() }
                    .environment(\.openRoute, OpenRouteAction { people.append($0) })
                    .tint(theme.colors.ink)
            }
            .badge(model.people.incoming.count)
            .accessibilityIdentifier("tab.people")

            Tab("Settings", systemImage: "gearshape") {
                NavigationStack { SettingsHome() }
                    .tint(theme.colors.ink)
            }
            .badge(model.account.deviceRequests.count)
            .accessibilityIdentifier("tab.settings")
        }
        .tabViewStyle(.sidebarAdaptable)
        .tabBarMinimizeBehavior(.onScrollDown)
        .tint(theme.colors.accentStrong)
    }
}
