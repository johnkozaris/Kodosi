import KodosiBridge
import KodosiDesign
import SwiftUI

struct FriendsSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        ListGroup {
            NavigationLink(value: PeopleRoute()) {
                ListRow(String(localized: "Friends"), subtitle: people.facts.isEmpty ? nil : people.facts,
                        symbol: "person.2.fill", tint: TileTint.blue)
                {
                    HStack(spacing: 10) {
                        if !people.incoming.isEmpty {
                            CountBadge(count: people.incoming.count).transition(AnyTransition.pop)
                        }
                        Disclosure()
                    }
                }
            }
            .buttonStyle(ListRowButtonStyle())
            .accessibilityIdentifier("settings.friends")
        }
        .animation(theme.motion.snappy, value: people.incoming.count)
    }
}
