import KodosiDesign
import SwiftUI

enum AppTab: Hashable {
    case terminals
    case rooms
    case people
    case settings
}

struct MainTabView: View {
    @Environment(AppModel.self) private var model
    @State private var selection: AppTab = .terminals

    var body: some View {
        TabView(selection: $selection) {
            Tab("Terminals", systemImage: "terminal", value: AppTab.terminals) {
                NavigationStack { TerminalsHome() }
            }
            .badge(model.terminals.needsYou.count)
            .accessibilityIdentifier("tab.terminals")

            Tab("Rooms", systemImage: "square.grid.2x2", value: AppTab.rooms) {
                NavigationStack { RoomsHome() }
            }
            .badge(model.rooms.invitations.count)
            .accessibilityIdentifier("tab.rooms")

            Tab("People", systemImage: "person.2", value: AppTab.people) {
                NavigationStack { PeopleHome() }
            }
            .badge(model.people.incoming.count)
            .accessibilityIdentifier("tab.people")

            Tab("Settings", systemImage: "gearshape", value: AppTab.settings) {
                NavigationStack { SettingsHome() }
            }
            .badge(model.account.deviceRequests.count)
            .accessibilityIdentifier("tab.settings")
        }
        .tabViewStyle(.sidebarAdaptable)
        .tabBarMinimizeBehavior(.onScrollDown)
    }
}
