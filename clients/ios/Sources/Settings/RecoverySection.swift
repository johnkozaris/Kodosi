import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit
import UniformTypeIdentifiers

struct RecoverySection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingReplacement = false
    @State private var replacing = false

    private var account: AccountStore {
        model.account
    }

    private var keyPresented: Binding<Bool> {
        Binding(
            get: { account.newRecoveryKey != nil },
            set: { presented in
                if !presented {
                    account.dismissRecoveryKey()
                }
            }
        )
    }

    var body: some View {
        Section {
            Button {
                if account.hasRecoveryKey {
                    confirmingReplacement = true
                } else {
                    replacing = false
                    account.createRecoveryKey()
                }
            } label: {
                SettingRow(
                    symbol: "key.fill",
                    tint: TileTint.amber,
                    title: Text(account.hasRecoveryKey ? "Make a new key…" : "Make a recovery key")
                ) {
                    if account.creatingRecoveryKey {
                        ProgressView()
                            .transition(.opacity)
                    }
                }
            }
            .disabled(account.creatingRecoveryKey)
            .raisedRows()
            .accessibilityIdentifier("settings.devices.recoveryKey")
            .confirmationDialog("Make a new recovery key?", isPresented: $confirmingReplacement, titleVisibility: .visible) {
                Button("Make a new key") {
                    replacing = true
                    account.createRecoveryKey()
                }
                .accessibilityIdentifier("settings.devices.confirmRecoveryKey")
            } message: {
                Text("The recovery key that you have now stops working.")
            }
            .sheet(isPresented: keyPresented) {
                RecoveryKeySheet(key: account.newRecoveryKey ?? "", replacedEarlierKey: replacing) {
                    account.dismissRecoveryKey()
                }
            }
        } header: {
            GroupedListHeader(title: "If you lose all your devices")
        } footer: {
            Text("A recovery key approves a new device when you have no other device.")
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
        }
    }
}

struct RecoveryKeySheet: View {
    @Environment(\.theme) private var theme
    @State private var copied = false
    let key: String
    let replacedEarlierKey: Bool
    let done: () -> Void

    private var groups: [String] {
        key.split(separator: "-").map(String.init)
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                VStack(spacing: 12) {
                    IconTile(symbol: "key.fill", tint: TileTint.amber, size: 60)
                        .arrive()
                    Text("Your recovery key")
                        .appTextStyle(.title)
                        .foregroundStyle(theme.colors.ink)
                    Text("""
                    Keep it in a safe place, such as a password manager. \
                    It approves a new device when you have no other device. Kodosi does not show it again.
                    """)
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                }
                keyGrid
                if replacedEarlierKey {
                    Label("The key that you had before stops working.", systemImage: "exclamationmark.triangle.fill")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.caution)
                        .accessibilityIdentifier("recoveryKey.replaced")
                }
                VStack(spacing: 10) {
                    Button(action: copy) {
                        Label(copied ? String(localized: "Copied") : String(localized: "Copy"), systemImage: copied ? "checkmark" : "doc.on.doc")
                            .contentTransition(.symbolEffect(.replace))
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.kodosi(copied ? .tinted : .secondary, size: .large))
                    .accessibilityIdentifier("recoveryKey.copy")
                    Button(action: done) {
                        Text("I saved it").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.kodosi(.primary, size: .large))
                    .accessibilityIdentifier("recoveryKey.done")
                }
            }
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
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Recovery key"))
        .accessibilityValue(Text(key))
        .speechSpellsOutCharacters()
        .accessibilityIdentifier("recoveryKey.text")
        .contextMenu {
            Button(action: copy) {
                Label("Copy", systemImage: "doc.on.doc")
            }
        }
    }

    private func copy() {
        UIPasteboard.general.setItems(
            [[UTType.utf8PlainText.identifier: key]],
            options: [.localOnly: true, .expirationDate: Date.now.addingTimeInterval(180)]
        )
        copied = true
        AccessibilityNotification.Announcement(String(localized: "Recovery key copied")).post()
    }
}
