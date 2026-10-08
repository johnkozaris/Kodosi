import AppKit
import SwiftUI

struct VerificationCode: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let code: String
    let identifier: String

    var body: some View {
        HStack(spacing: 12) {
            Text(code)
                .font(.system(size: 26, weight: .semibold, design: .monospaced))
                .tracking(2)
                .foregroundStyle(theme.colors.primary)
                .textSelection(.enabled)
                .padding(.horizontal, 14).padding(.vertical, 8)
                .background(theme.colors.card, in: RoundedRectangle(cornerRadius: theme.radius.sm))
                .overlay(RoundedRectangle(cornerRadius: theme.radius.sm).stroke(theme.colors.border))
                .speechSpellsOutCharacters()
                .accessibilityLabel(Text("Code"))
                .accessibilityValue(Text(code))
                .accessibilityIdentifier(identifier)
            Button(copied ? String(localized: "Copied") : String(localized: "Copy")) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(code, forType: .string)
                copied = true
                AccessibilityNotification.Announcement(String(localized: "Code copied")).post()
            }
            .buttonStyle(SolidSecondaryButtonStyle())
            .accessibilityIdentifier("\(identifier).copy")
        }
        .onChange(of: code) { _, _ in copied = false }
    }
}
