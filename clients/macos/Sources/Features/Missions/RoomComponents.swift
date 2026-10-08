import SwiftUI

struct RoomSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            ForEach(0 ..< 4, id: \.self) { index in
                HStack(alignment: .top, spacing: 12) {
                    SkeletonBlock(width: 28, height: 28, radius: 14)
                    VStack(alignment: .leading, spacing: 8) {
                        SkeletonBlock(width: 90, height: 9)
                        SkeletonBlock(height: 10)
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 160 : 240, height: 10)
                    }
                }
            }
            Spacer()
        }
        .padding(24)
        .accessibilityElement(children: .ignore).accessibilityLabel(Text("Loading"))
    }
}

struct RoomRecovery: View {
    @Environment(\.theme) private var theme
    @State private var showsDetail = false
    let message: String
    let retry: () -> Void

    var body: some View {
        EmptyState(title: "This did not load", message: "Check your connection, then try again.") {
            Image(systemName: "wifi.exclamationmark").font(.system(size: 20, weight: .medium)).foregroundStyle(theme.colors.caution)
                .frame(width: 52, height: 52).background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        } actions: {
            Button("Details") { showsDetail.toggle() }.buttonStyle(.kodosi(.ghost))
                .popover(isPresented: $showsDetail) {
                    Text(message).appTextStyle(.body).textSelection(.enabled).padding(16)
                        .fixedSize(horizontal: false, vertical: true).popoverSheet(width: 340)
                }
            Button("Try again", action: retry).buttonStyle(.kodosi(.primary))
        }
    }
}
