import AppKit
import SwiftUI

struct VerificationCode: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let code: String
    let identifier: String

    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 5) {
                ForEach(Array(code.enumerated()), id: \.offset) { index, character in
                    if character == "-" || character == " " {
                        Capsule().fill(theme.colors.inkFaint.opacity(0.6)).frame(width: 8, height: 2)
                    } else {
                        Text(String(character))
                            .font(.system(size: 22, weight: .semibold, design: .monospaced))
                            .foregroundStyle(theme.colors.ink)
                            .frame(width: 30, height: 40)
                            .raised(Radius.sm, fill: theme.colors.lifted)
                            .arrive(delay: Double(index) * 0.03, rise: 6)
                    }
                }
            }
            .textSelection(.enabled)
            .accessibilityElement(children: .ignore)
            .speechSpellsOutCharacters()
            .accessibilityLabel(Text("Code"))
            .accessibilityValue(Text(code))
            .accessibilityIdentifier(identifier)
            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(code, forType: .string)
                copied = true
                AccessibilityNotification.Announcement(String(localized: "Code copied")).post()
            } label: {
                Label(copied ? String(localized: "Copied") : String(localized: "Copy"), systemImage: copied ? "checkmark" : "doc.on.doc")
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.kodosi(copied ? .tinted : .secondary))
            .accessibilityIdentifier("\(identifier).copy")
        }
        .onChange(of: code) { _, _ in copied = false }
    }
}
