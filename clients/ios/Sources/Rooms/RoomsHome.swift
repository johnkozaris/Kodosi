import KodosiDesign
import SwiftUI

struct RoomsHome: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            EmptyState(title: "Rooms", message: "Rooms you join appear here.") {
                IconTile(symbol: "square.grid.2x2", size: 52)
            } actions: {
                EmptyView()
            }
            .padding(.top, 80)
        }
        .navigationTitle("Rooms")
    }
}
