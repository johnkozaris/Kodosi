#if canImport(AppKit)
    import AppKit
#else
    import UIKit
    import UniformTypeIdentifiers
#endif
import SwiftUI

public struct RecoveryKeySheet: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let key: String
    let done: () -> Void

    public init(key: String, done: @escaping () -> Void) {
        self.key = key
        self.done = done
    }

    private var groups: [String] {
        key.split(separator: "-").map(String.init)
    }

    public var body: some View {
        #if os(iOS)
            ScrollView {
                content
                    .frame(maxWidth: 440)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 24)
                    .padding(.top, 36)
                    .padding(.bottom, 24)
            }
            .scrollBounceBehavior(.basedOnSize)
            .background { GroundBackdrop() }
            .presentationDetents([.large])
            .presentationBackground(theme.colors.ground)
            .interactiveDismissDisabled()
        #else
            content.padding(28).frame(width: 420)
        #endif
    }

    private var content: some View {
        VStack(spacing: 24) {
            VStack(spacing: 12) {
                IconTile(symbol: "key.fill", tint: TileTint.amber, size: 60)
                    .arrive()
                Text("Your recovery key", bundle: .module)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("""
                Keep it in a safe place, such as a password manager. \
                It approves a new device when you have no other device. Kodosi does not show it again.
                """, bundle: .module)
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            keyGrid
            VStack(spacing: 10) {
                Button(action: copy) {
                    Label(
                        copied ? String(localized: "Copied", bundle: .module) : String(localized: "Copy", bundle: .module),
                        systemImage: copied ? "checkmark" : "doc.on.doc"
                    )
                    .contentTransition(.symbolEffect(.replace))
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(copied ? .tinted : .secondary, size: .large))
                .accessibilityIdentifier("recoveryKey.copy")
                Button(action: done) {
                    Text("I saved it", bundle: .module).frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(.primary, size: .large))
                .keyboardShortcut(.defaultAction)
                .accessibilityIdentifier("recoveryKey.done")
            }
        }
        .animation(theme.motion.snappy, value: copied)
        .sensoryFeedback(.success, trigger: copied) { _, copied in copied }
    }

    private var keyGrid: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4), spacing: 8) {
            ForEach(Array(groups.enumerated()), id: \.offset) { index, group in
                Text(group)
                    .font(.system(.title3, design: .monospaced, weight: .semibold))
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                    .foregroundStyle(theme.colors.ink)
                    .frame(maxWidth: .infinity, minHeight: 46)
                    .raised(Radius.md, fill: theme.colors.lifted)
                    .arrive(delay: 0.12 + Double(index) * 0.04, rise: 6)
            }
        }
        .padding(12)
        .well(Radius.xl)
        .textSelection(.enabled)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Recovery key", bundle: .module))
        .accessibilityValue(Text(key))
        .speechSpellsOutCharacters()
        .accessibilityIdentifier("recoveryKey.text")
        .contextMenu {
            Button(action: copy) {
                Label { Text("Copy", bundle: .module) } icon: { Image(systemName: "doc.on.doc") }
            }
        }
    }

    private func copy() {
        #if canImport(AppKit)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(key, forType: .string)
            NSPasteboard.general.setData(Data(), forType: NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType"))
        #else
            UIPasteboard.general.setItems(
                [[UTType.utf8PlainText.identifier: key]],
                options: [.localOnly: true, .expirationDate: Date.now.addingTimeInterval(180)]
            )
        #endif
        copied = true
        AccessibilityNotification.Announcement(String(localized: "Recovery key copied", bundle: .module)).post()
    }
}
