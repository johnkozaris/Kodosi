import SwiftUI

struct ProgressCaption: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small).accessibilityHidden(true)
            Text(text).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
        }
    }
}

struct ErrorNote: View {
    @Environment(\.theme) private var theme
    let message: String

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.triangle").accessibilityHidden(true)
            Text(message).appTextStyle(.caption).textSelection(.enabled)
        }
        .foregroundStyle(theme.colors.destructive)
    }
}
