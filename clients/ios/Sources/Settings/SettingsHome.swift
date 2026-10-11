import KodosiDesign
import SwiftUI

struct SettingsHome: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            EmptyState(title: "Settings", message: "") {
                IconTile(symbol: "gearshape", size: 52)
            } actions: {
                EmptyView()
            }
            .padding(.top, 80)
        }
        .navigationTitle("Settings")
    }
}
