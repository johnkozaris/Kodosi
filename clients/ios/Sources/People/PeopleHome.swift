import KodosiDesign
import SwiftUI

struct PeopleHome: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            EmptyState(title: "People", message: "Friends you add appear here.") {
                IconTile(symbol: "person.2", size: 52)
            } actions: {
                EmptyView()
            }
            .padding(.top, 80)
        }
        .navigationTitle("People")
    }
}
