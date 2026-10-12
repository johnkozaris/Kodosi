import KodosiDesign
import SwiftUI
import UIKit

struct CodeTiles: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let code: String
    let identifier: String

    private var groups: [String] {
        code.split(whereSeparator: { $0 == "-" || $0 == " " }).map(String.init)
    }

    var body: some View {
        VStack(spacing: 14) {
            HStack(spacing: 8) {
                ForEach(Array(groups.enumerated()), id: \.offset) { index, group in
                    Text(group)
                        .font(.system(size: 26, weight: .semibold, design: .monospaced))
                        .minimumScaleFactor(0.6)
                        .lineLimit(1)
                        .foregroundStyle(theme.colors.ink)
                        .padding(.horizontal, 12)
                        .frame(minHeight: 54)
                        .raised(Radius.md, fill: theme.colors.lifted)
                        .arrive(delay: Double(index) * 0.06, rise: 6)
                }
            }
            .textSelection(.enabled)
            .accessibilityElement(children: .ignore)
            .speechSpellsOutCharacters()
            .accessibilityLabel(Text("Code"))
            .accessibilityValue(Text(code))
            .accessibilityIdentifier(identifier)
            Button {
                UIPasteboard.general.string = code
                copied = true
                AccessibilityNotification.Announcement(String(localized: "Code copied")).post()
            } label: {
                Label(copied ? String(localized: "Copied") : String(localized: "Copy"), systemImage: copied ? "checkmark" : "doc.on.doc")
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.kodosi(copied ? .tinted : .secondary, size: .small))
            .accessibilityIdentifier("\(identifier).copy")
        }
        .animation(theme.motion.snappy, value: copied)
        .sensoryFeedback(.success, trigger: copied) { _, copied in copied }
        .onChange(of: code) { _, _ in copied = false }
    }
}
