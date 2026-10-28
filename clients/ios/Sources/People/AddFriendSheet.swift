import KodosiBridge
import KodosiDesign
import SwiftUI

struct AddFriendSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var trusting = false
    @FocusState private var focused: Bool

    private var trimmed: String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 26) {
                    VStack(spacing: 14) {
                        TogetherArt(size: 52, ring: theme.colors.ground)
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
                    }
                    .arrive(delay: 0.08)
                    divider
                        .arrive(delay: 0.12)
                    VStack(spacing: 10) {
                        Text("Send them your invite. They add it to send you a request.")
                            .appTextStyle(.footnote)
                            .foregroundStyle(theme.colors.inkMuted)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                        InviteShareButton(identifier: "people.addFriend.shareInvite") {
                            Label("Share my invite", systemImage: "square.and.arrow.up")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.kodosi(.secondary, size: .large))
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
            .background { GroundBackdrop() }
            .navigationTitle("Add a friend")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(role: .close) { dismiss() }
                        .accessibilityIdentifier("people.addFriend.close")
                }
            }
        }
        .presentationDetents([.large])
        .presentationBackground(theme.colors.ground)
        .friendTrustAlert(isPresented: $trusting, name: PeopleStore.handle(in: trimmed).map { "@\($0)" }, confirm: confirm)
        .task {
            if model.people.ownInvite == nil {
                model.people.requestInvite()
            }
        }
    }

    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: "person.badge.plus")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(focused ? theme.colors.accentStrong : theme.colors.inkFaint)
                .accessibilityHidden(true)
            TextField("Their invite or username", text: $text)
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
                Button { text = "" } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 17))
                        .foregroundStyle(theme.colors.inkFaint)
                        .frame(width: 36, height: 36)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Clear"))
                .accessibilityIdentifier("people.username.clear")
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
        trusting = true
    }

    private func confirm() {
        guard model.people.add(trimmed) else { return }
        dismiss()
    }
}
