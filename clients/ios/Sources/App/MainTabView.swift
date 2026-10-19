import KodosiBridge
import SwiftUI

struct MainTabView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        TabView {
            Tab("Terminals", systemImage: "terminal") {
                NavigationStack { TerminalsHome() }
            }
            .badge(model.terminals.needsYou.count)
            .accessibilityIdentifier("tab.terminals")

            Tab("Rooms", systemImage: "square.grid.2x2") {
                NavigationStack { RoomsHome() }
            }
            .badge(model.rooms.invitations.count + model.rooms.missions.count(where: \.unread))
            .accessibilityIdentifier("tab.rooms")

            Tab("Friends", systemImage: "person.2") {
                NavigationStack { PeopleHome() }
            }
            .badge(model.people.incoming.count)
            .accessibilityIdentifier("tab.people")

            Tab("Settings", systemImage: "gearshape") {
                NavigationStack { SettingsHome() }
            }
            .badge(model.account.deviceRequests.count)
            .accessibilityIdentifier("tab.settings")
        }
        .tabViewStyle(.sidebarAdaptable)
        .tabBarMinimizeBehavior(.onScrollDown)
    }
}
