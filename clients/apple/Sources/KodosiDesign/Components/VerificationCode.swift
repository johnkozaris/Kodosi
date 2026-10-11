#if canImport(AppKit)
    import AppKit
#else
    import UIKit
#endif
import SwiftUI

public struct VerificationCode: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    public let code: String
    public let identifier: String

    public init(code: String, identifier: String) {
        self.code = code
        self.identifier = identifier
    }

    public var body: some View {
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
            .accessibilityLabel(Text("Code", bundle: .module))
            .accessibilityValue(Text(code))
            .accessibilityIdentifier(identifier)
            Button {
                #if canImport(AppKit)
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(code, forType: .string)
                #else
                    UIPasteboard.general.string = code
                #endif
                copied = true
                AccessibilityNotification.Announcement(String(localized: "Code copied", bundle: .module)).post()
            } label: {
                Label(
                    copied ? String(localized: "Copied", bundle: .module) : String(localized: "Copy", bundle: .module),
                    systemImage: copied ? "checkmark" : "doc.on.doc"
                )
                .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.kodosi(copied ? .tinted : .secondary))
            .accessibilityIdentifier("\(identifier).copy")
        }
        .onChange(of: code) { _, _ in copied = false }
    }
}
