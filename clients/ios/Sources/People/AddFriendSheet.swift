import KodosiBridge
import KodosiDesign
import SwiftUI
import VisionKit

struct AddFriendSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var text: String
    @State private var trusting: FriendTrust?
    @State private var showingInvite = false
    @State private var scanning = false
    @FocusState private var focused: Bool
    @ScaledMetric(relativeTo: .body) private var glyph: CGFloat = 15
    @State private var opened: String?

    init(invite: String? = nil) {
        _opened = State(initialValue: invite)
        _text = State(initialValue: invite ?? "")
    }

    private var trimmed: String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 26) {
                    VStack(spacing: 14) {
                        TogetherArt(size: 52)
                            .scaleEffect(0.8)
                            .arrive()
                        Text("Friends can open the terminals you share, and you can open theirs.")
                            .appTextStyle(.callout)
                            .foregroundStyle(theme.colors.inkMuted)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                            .arrive(delay: 0.04)
                    }
                    VStack(spacing: 12) {
                        field
                        Button(action: add) {
                            Text("Add friend").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.kodosi(.primary, size: .large))
                        .disabled(trimmed.isEmpty)
                        .accessibilityIdentifier("people.addFriend")
                        if DataScannerViewController.isSupported {
                            Button { scanning = true } label: {
                                Label("Scan their invite", systemImage: "qrcode.viewfinder")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.kodosi(.secondary, size: .large))
                            .accessibilityIdentifier("people.addFriend.scan")
                        }
                    }
                    .arrive(delay: 0.08)
                    divider
                        .arrive(delay: 0.12)
                    VStack(spacing: 10) {
                        Text("Show them your invite, or share it. They add it to send you a request.")
                            .appTextStyle(.footnote)
                            .foregroundStyle(theme.colors.inkMuted)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                        Button { showingInvite = true } label: {
                            Label("Show my invite", systemImage: "qrcode")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.kodosi(.secondary, size: .large))
                        .accessibilityIdentifier("people.addFriend.showInvite")
                    }
                    .arrive(delay: 0.16)
                }
                .frame(maxWidth: 460)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 24)
                .padding(.vertical, 16)
            }
            .scrollBounceBehavior(.basedOnSize)
            .scrollDismissesKeyboard(.interactively)
            .background(theme.colors.surface)
            .navigationTitle("Add a friend")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Close", symbol: "xmark", identifier: "people.addFriend.close", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.large])
        .kodosiSheet()
        .friendTrust($trusting) { _ in confirm() }
        .sheet(isPresented: $showingInvite) { InviteCodeSheet() }
        .sheet(isPresented: $scanning) {
            InviteScanSheet { invite in
                scanning = false
                text = invite
                trusting = .add(invite)
            }
        }
        .task {
            if let opened {
                self.opened = nil
                trusting = .add(opened)
            }
            if model.people.ownInvite == nil {
                model.people.requestInvite()
            }
        }
    }

    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: "person.badge.plus")
                .font(.system(size: glyph, weight: .semibold))
                .foregroundStyle(focused ? theme.colors.accentStrong : theme.colors.inkFaint)
                .accessibilityHidden(true)
            TextField("Their invite or username", text: $text)
                .textFieldStyle(.plain)
                .tint(theme.colors.accent)
                .appTextStyle(.body)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.send)
                .onSubmit(add)
                .focused($focused)
                .accessibilityIdentifier("people.username")
            if text.isEmpty {
                PasteButton(payloadType: String.self) { @Sendable values in
                    guard let value = values.first else { return }
                    Task { @MainActor in text = value }
                }
                .labelStyle(.iconOnly)
                .buttonBorderShape(.capsule)
                .tint(theme.colors.accent)
                .accessibilityIdentifier("people.paste")
            } else {
                IconButton(title: "Clear", symbol: "xmark", identifier: "people.username.clear", size: 30) { text = "" }
            }
        }
        .padding(.leading, 18)
        .padding(.trailing, 8)
        .frame(minHeight: 54)
        .background {
            RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: focused ? .lifted : .resting)
        }
        .overlay {
            Capsule().strokeBorder(theme.colors.accent.opacity(focused ? 0.5 : 0), lineWidth: 1.5)
        }
        .animation(theme.motion.hover, value: focused)
        .animation(theme.motion.snappy, value: text.isEmpty)
    }

    private var divider: some View {
        HStack(spacing: 12) {
            Capsule().fill(theme.colors.hairline).frame(height: 1)
            Text("or")
                .appTextStyle(.caption)
                .foregroundStyle(theme.colors.inkFaint)
            Capsule().fill(theme.colors.hairline).frame(height: 1)
        }
        .accessibilityHidden(true)
    }

    private func add() {
        guard !trimmed.isEmpty else { return }
        trusting = .add(trimmed)
    }

    private func confirm() {
        guard model.people.add(trimmed) else { return }
        dismiss()
    }
}
