import SwiftUI

struct HistoryMessageView: View {
    @Environment(\.theme) private var theme
    let entry: ConversationEntry
    @State private var expanded = false

    private var isLarge: Bool {
        entry.content.count > 2000
    }

    private var isTool: Bool {
        entry.toolName != nil || entry.role == "tool"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(entry.toolName ?? (entry.role == "user" ? String(localized: "You") : entry.role.capitalized))
                    .appTextStyle(.headingItem).foregroundStyle(theme.colors.primary)
                Spacer()
                if isLarge || isTool {
                    Button(expanded ? "Collapse" : "Expand") { expanded.toggle() }.buttonStyle(.plain)
                        .appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                }
            }
            if !isTool || expanded {
                Text(expanded || !isLarge ? entry.content : String(entry.content.prefix(2000)) + "…")
                    .appTextStyle(.body).textSelection(.enabled).lineSpacing(4)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text(String(entry.content.prefix(160))).appTextStyle(.monoCaption).lineLimit(2)
                    .foregroundStyle(theme.colors.mutedForeground)
            }
        }.padding(14)
            .background(entry.role == "user" ? theme.colors.surfacePanel : theme.colors.background)
            .overlay(alignment: .leading) {
                if entry.role == "user" {
                    Rectangle().fill(theme.colors.primary.opacity(0.5)).frame(width: 2)
                }
            }
    }
}
