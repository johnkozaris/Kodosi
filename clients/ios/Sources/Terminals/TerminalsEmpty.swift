import KodosiDesign
import SwiftUI

struct TerminalsEmpty: View {
    var body: some View {
        EmptyState(
            title: "Your terminals run on your computers",
            message: """
            Get Kodosi for your Mac, Linux or Windows computer at kodosi.com. Start a terminal there, \
            or ask a friend to share one. It shows up here, so you can follow its work and answer its agents.
            """,
        ) {
            TerminalsArt()
        } actions: {
            EmptyView()
        }
        .accessibilityIdentifier("terminals.empty")
    }
}

private struct TerminalsArt: View {
    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            ArtTile().terminalScope()
            AgentMark(kind: .claude, size: 34, activity: .working)
                .padding(4)
                .offset(x: 14, y: 14)
        }
        .padding(.trailing, 14)
        .padding(.bottom, 14)
        .accessibilityHidden(true)
    }
}

private struct ArtTile: View {
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 4) {
                ForEach(0 ..< 3, id: \.self) { _ in
                    Circle()
                        .fill(theme.colors.hairline)
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
        .terminalTile()
    }
}

struct TerminalsLoading: View {
    var body: some View {
        VStack(spacing: 10) {
            ForEach(0 ..< 4, id: \.self) { index in
                HStack(spacing: 12) {
                    SkeletonBlock(width: 34, height: 34, radius: 10)
                    VStack(alignment: .leading, spacing: 7) {
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 150 : 120, height: 12)
                        SkeletonBlock(width: index.isMultiple(of: 2) ? 96 : 130, height: 10)
                    }
                    Spacer()
                }
                .padding(.horizontal, 14)
                .frame(height: 68)
                .raised(Radius.lg)
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 16)
        .frame(maxHeight: .infinity, alignment: .top)
        .accessibilityElement()
        .accessibilityLabel(Text("Loading terminals"))
    }
}
