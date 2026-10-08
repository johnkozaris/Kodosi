import SwiftUI

struct RoomAvatar: View {
    @Environment(\.theme) private var theme
    let name: String
    var size: CGFloat = 28
    var body: some View {
        Text(name.split(separator: " ").prefix(2).compactMap(\.first).map(String.init).joined().uppercased())
            .font(.system(size: size * 0.36, weight: .semibold))
            .foregroundStyle(theme.colors.primary).frame(width: size, height: size)
            .background(theme.colors.secondary, in: Circle()).accessibilityLabel(name)
    }
}

struct RoomSkeleton: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var bright = false
    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            ForEach(0 ..< 4) { index in
                HStack(alignment: .top, spacing: 12) {
                    Circle().frame(width: 28, height: 28)
                    VStack(alignment: .leading, spacing: 9) {
                        RoundedRectangle(cornerRadius: 4).frame(width: 90, height: 9)
                        RoundedRectangle(cornerRadius: 4).frame(height: 10)
                        RoundedRectangle(cornerRadius: 4).frame(maxWidth: index.isMultiple(of: 2) ? 160 : 240).frame(height: 10)
                    }
                }
            }
            Spacer()
        }.padding(24).foregroundStyle(theme.colors.muted.opacity(bright ? 0.65 : 0.3))
            .accessibilityElement(children: .ignore).accessibilityLabel(Text("Loading"))
            .onAppear {
                if !reduceMotion {
                    withAnimation(.easeInOut(duration: 0.9).repeatForever()) { bright = true }
                }
            }
    }
}

struct RoomRecovery: View {
    @Environment(\.theme) private var theme
    let message: String
    let retry: () -> Void
    var body: some View {
        VStack(spacing: 14) {
            Image(systemName: "wifi.exclamationmark").font(.system(size: 32)).foregroundStyle(theme.colors.statusWaiting)
            Button("Try again", action: retry).buttonStyle(SolidPrimaryButtonStyle())
            DisclosureGroup("Details") { Text(message).textSelection(.enabled).appTextStyle(.caption) }
                .foregroundStyle(theme.colors.mutedForeground).frame(maxWidth: 300)
        }.frame(maxWidth: .infinity, maxHeight: .infinity).padding(24)
    }
}

struct RoomActionError: View {
    @Environment(\.theme) private var theme
    let message: String
    let dismiss: () -> Void
    @State private var expanded = false
    var body: some View {
        HStack {
            Button { expanded.toggle() } label: { Label("Action failed", systemImage: "exclamationmark.circle") }
                .buttonStyle(.plain).popover(isPresented: $expanded) { Text(message).textSelection(.enabled).padding(16).frame(maxWidth: 360) }
            Spacer()
            Button(action: dismiss) { Image(systemName: "xmark") }.buttonStyle(.plain).accessibilityLabel(Text("Dismiss error"))
        }.appTextStyle(.caption).foregroundStyle(theme.colors.destructive).padding(.horizontal, 16).padding(.vertical, 8)
    }
}
