import SwiftUI

struct RecoveryKeySheet: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let key: String
    let done: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "key.fill").font(.system(size: 26, weight: .semibold)).foregroundStyle(theme.colors.accent)
            Text("Your recovery key").appTextStyle(.title).foregroundStyle(theme.colors.ink)
            Text("Keep it in a safe place, such as a password manager. It approves a new device when you have no other device. Kodosi does not show it again.")
                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            Text(lines)
                .font(.system(size: 17, weight: .semibold, design: .monospaced)).foregroundStyle(theme.colors.ink)
                .multilineTextAlignment(.center).lineSpacing(6).textSelection(.enabled)
                .padding(.horizontal, 18).padding(.vertical, 14).frame(maxWidth: .infinity)
                .raised(Radius.lg, fill: theme.colors.lifted)
                .speechSpellsOutCharacters()
                .accessibilityLabel(Text("Recovery key")).accessibilityValue(Text(key))
                .accessibilityIdentifier("recoveryKey.text")
            HStack(spacing: 8) {
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(key, forType: .string)
                    copied = true
                    AccessibilityNotification.Announcement(String(localized: "Recovery key copied")).post()
                } label: {
                    Label(copied ? String(localized: "Copied") : String(localized: "Copy"), systemImage: copied ? "checkmark" : "doc.on.doc")
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(.kodosi(copied ? .tinted : .secondary))
                .accessibilityIdentifier("recoveryKey.copy")
                Button("I saved it", action: done)
                    .buttonStyle(.kodosi(.primary))
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("recoveryKey.done")
            }
        }
        .padding(28).frame(width: 420)
    }

    private var lines: String {
        let groups = key.split(separator: "-")
        let half = (groups.count + 1) / 2
        return [groups.prefix(half), groups.dropFirst(half)].map { $0.joined(separator: "-") }.joined(separator: "\n")
    }
}
