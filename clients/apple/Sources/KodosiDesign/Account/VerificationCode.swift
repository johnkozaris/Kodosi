#if canImport(AppKit)
    import AppKit
#else
    import UIKit
#endif
import SwiftUI

struct VerificationCode: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let code: String
    let identifier: String

    init(code: String, identifier: String) {
        self.code = code
        self.identifier = identifier
    }

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
            .accessibilityLabel(Text("Code", bundle: .module))
            .accessibilityValue(Text(code))
            .accessibilityIdentifier(identifier)
            Button(action: copy) {
                Label(
                    copied ? String(localized: "Copied", bundle: .module) : String(localized: "Copy", bundle: .module),
                    systemImage: copied ? "checkmark" : "doc.on.doc"
                )
                .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.kodosi(copied ? .tinted : .secondary, size: .small))
            .accessibilityIdentifier("\(identifier).copy")
        }
        .animation(theme.motion.snappy, value: copied)
        .sensoryFeedback(.success, trigger: copied) { _, copied in copied }
        .onChange(of: code) { _, _ in copied = false }
    }

    private func copy() {
        #if canImport(AppKit)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(code, forType: .string)
        #else
            UIPasteboard.general.string = code
        #endif
        copied = true
        AccessibilityNotification.Announcement(String(localized: "Code copied", bundle: .module)).post()
    }
}
