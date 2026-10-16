import SwiftUI

public struct RoomSkeleton: View {
    public init() {}

    public var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            ForEach(0 ..< 4, id: \.self) { index in
                HStack(alignment: .top, spacing: 12) {
                    SkeletonBlock(width: 30, height: 30, radius: 15)
                    VStack(alignment: .leading, spacing: 8) {
                        SkeletonBlock(width: 96, height: 10)
                        SkeletonBlock(height: 11)
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 170 : 240, height: 11)
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Loading", bundle: .module))
    }
}

public struct RoomRecovery: View {
    @Environment(\.theme) private var theme
    let message: String
    let retry: () -> Void

    public init(message: String, retry: @escaping () -> Void) {
        self.message = message
        self.retry = retry
    }

    public var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "wifi.exclamationmark")
                .font(.system(size: 22, weight: .medium))
                .foregroundStyle(theme.colors.caution)
                .frame(width: 56, height: 56)
                .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: 17, style: .continuous))
                .accessibilityHidden(true)
                .arrive()
            VStack(spacing: 6) {
                Text("This did not load", bundle: .module)
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                Text(verbatim: message)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }
            .arrive(delay: 0.05)
            Button(action: retry) { Text("Try again", bundle: .module) }
                .buttonStyle(.kodosi(.primary))
                .accessibilityIdentifier("rooms.retry")
                .arrive(delay: 0.1)
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

public struct TerminalGhost: View {
    @Environment(\.theme) private var theme

    public init() {}

    public var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "chevron.right").font(.system(size: 16, weight: .heavy)).foregroundStyle(theme.colors.inkFaint)
            CursorBlock(width: 11, height: 20)
            Spacer()
        }
        .padding(16).frame(width: 190, height: 96, alignment: .topLeading)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                .strokeBorder(theme.colors.inkFaint.opacity(0.5), style: StrokeStyle(lineWidth: 1.2, dash: [5, 5]))
        }
        .accessibilityHidden(true)
    }
}
