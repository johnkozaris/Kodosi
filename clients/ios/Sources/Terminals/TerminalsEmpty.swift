import KodosiDesign
import SwiftUI

struct TerminalsEmpty: View {
    var body: some View {
        EmptyState(
            title: "Your terminals live on your computers",
            message: "Start a terminal in Kodosi on your Mac, or ask a friend to share one. It shows up here, so you can follow its work and answer its agents."
        ) {
            TerminalsArt()
        } actions: {
            EmptyView()
        }
        .accessibilityIdentifier("terminals.empty")
    }
}

private struct TerminalsArt: View {
    @Environment(\.theme) private var theme

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 4) {
                    ForEach(0 ..< 3, id: \.self) { _ in
                        Circle()
                            .fill(theme.colors.inkFaint.opacity(0.45))
                            .frame(width: 6, height: 6)
                    }
                }
                HStack(spacing: 5) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 12, weight: .heavy, design: .monospaced))
                        .foregroundStyle(theme.colors.inkMuted)
                    CursorBlock(width: 8, height: 15)
                }
            }
            .padding(14)
            .frame(width: 128, height: 84, alignment: .topLeading)
            .background(theme.colors.terminal, in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                    .strokeBorder(theme.colors.hairline.opacity(0.8), lineWidth: 0.5)
            }
            .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.15), radius: 14, x: 4, y: 10)
            AgentMark(kind: .claude, size: 34, activity: .working)
                .padding(4)
                .offset(x: 14, y: 14)
        }
        .padding(.trailing, 14)
        .padding(.bottom, 14)
        .accessibilityHidden(true)
    }
}

struct TerminalsLoading: View {
    var body: some View {
        VStack(spacing: 18) {
            ForEach(0 ..< 4, id: \.self) { index in
                HStack(spacing: 12) {
                    SkeletonBlock(width: 36, height: 36, radius: 11)
                    VStack(alignment: .leading, spacing: 7) {
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 150 : 120, height: 12)
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 96 : 130, height: 10)
                    }
                    Spacer()
                }
            }
        }
        .padding(.horizontal, 28)
        .padding(.top, 24)
        .frame(maxHeight: .infinity, alignment: .top)
        .accessibilityElement()
        .accessibilityLabel(Text("Loading terminals"))
    }
}
