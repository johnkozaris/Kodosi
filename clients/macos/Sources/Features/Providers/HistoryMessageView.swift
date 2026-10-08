import SwiftUI

struct HistoryMessageView: View {
    @Environment(\.theme) private var theme
    let entry: ConversationEntry
    let agent: AgentKind
    @State private var expanded = false

    private var isLarge: Bool {
        entry.content.count > 2000
    }

    private var isTool: Bool {
        entry.toolName != nil || entry.role == "tool"
    }

    private var isUser: Bool {
        entry.role == "user"
    }

    var body: some View {
        if isTool {
            tool
        } else if isUser {
            HStack {
                Spacer(minLength: 60)
                text.padding(.horizontal, 13).padding(.vertical, 9)
                    .background {
                        UnevenRoundedRectangle(
                            topLeadingRadius: 16, bottomLeadingRadius: 16, bottomTrailingRadius: 6, topTrailingRadius: 16, style: .continuous
                        )
                        .fill(theme.colors.accentSoft)
                    }
            }
        } else {
            HStack(alignment: .top, spacing: 10) {
                AgentMark(kind: agent, size: 24)
                text.frame(maxWidth: .infinity, alignment: .leading).padding(.top, 3)
            }
        }
    }

    private var text: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(expanded || !isLarge ? entry.content : String(entry.content.prefix(2000)) + "…")
                .appTextStyle(.body).lineSpacing(3).foregroundStyle(theme.colors.ink).textSelection(.enabled)
            if isLarge {
                Button(expanded ? "Show less" : "Show all") { expanded.toggle() }.buttonStyle(.kodosi(.ghost, size: .small))
            }
        }
    }

    private var tool: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button { withAnimation(theme.motion.spring) { expanded.toggle() } } label: {
                HStack(spacing: 6) {
                    Image(systemName: "wrench.and.screwdriver").font(.system(size: 9, weight: .semibold))
                    Text(entry.toolName ?? String(localized: "Tool")).lineLimit(1)
                    Image(systemName: "chevron.right").font(.system(size: 7, weight: .bold)).rotationEffect(.degrees(expanded ? 90 : 0))
                }
                .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                .padding(.horizontal, 9).frame(height: 22).background(theme.colors.ink.opacity(0.07), in: Capsule())
            }
            .buttonStyle(.plain)
            if expanded {
                Text(isLarge ? String(entry.content.prefix(4000)) : entry.content)
                    .appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                    .padding(10).frame(maxWidth: .infinity, alignment: .leading).well(Radius.md)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .padding(.leading, 34)
    }
}
