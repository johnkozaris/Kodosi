import KodosiDesign
import SwiftUI

struct TerminalsHome: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            EmptyState(title: "Terminals", message: "Your terminals appear here when a computer shares them with you.") {
                IconTile(symbol: "terminal", size: 52)
            } actions: {
                EmptyView()
            }
            .padding(.top, 80)
        }
        .navigationTitle("Terminals")
    }
}
