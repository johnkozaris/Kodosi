import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomCreateSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let created: (String) -> Void
    @State private var name = ""
    @State private var invited: Set<String> = []
    @State private var creating = false
    @State private var failure: String?
    @State private var made = 0
    @FocusState private var focused: Bool

    private var valid: Bool {
        ProductInput.validName(name)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 22) {
                    VStack(spacing: 18) {
                        RoomSigil(key: name.isEmpty ? "new" : name, size: 68)
                            .animation(theme.motion.spring, value: name)
                        TextField("Launch week", text: $name)
                            .textFieldStyle(WellTextFieldStyle(radius: Radius.lg))
                            .focused($focused)
                            .submitLabel(.done)
                            .onSubmit(create)
                            .accessibilityLabel(Text("Room name"))
                            .accessibilityIdentifier("rooms.create.name")
                        Text("Everyone in the room shares its conversation. Your terminals in it go to your friends.")
                            .appTextStyle(.caption).fontWeight(.regular)
                            .foregroundStyle(theme.colors.inkFaint)
                            .multilineTextAlignment(.center)
                    }
                    friends
                    if let failure {
                        ErrorNote(message: failure).transition(AnyTransition.rise)
                    }
                }
                .padding(20)
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
            }
            .background(theme.colors.surface)
            .navigationTitle("New room")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .buttonStyle(.kodosi(.ghost, size: .small))
                        .fixedSize()
                        .accessibilityIdentifier("rooms.create.cancel")
                }
                .sharedBackgroundVisibility(.hidden)
                ToolbarItem(placement: .confirmationAction) {
                    Button(action: create) {
                        Text(creating ? "Making…" : "Make room").shimmer(creating)
                    }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .fixedSize()
                    .disabled(!valid || creating)
                    .accessibilityIdentifier("rooms.create.confirm")
                }
                .sharedBackgroundVisibility(.hidden)
            }
            .animation(theme.motion.spring, value: failure)
            .animation(theme.motion.snappy, value: invited)
        }
        .presentationDetents([.medium, .large])
        .kodosiSheet()
        .interactiveDismissDisabled(creating)
        .sensoryFeedback(.success, trigger: made)
        .sensoryFeedback(.selection, trigger: invited)
        .onAppear { focused = true }
    }

    private var friends: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Invite friends").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
            if model.people.friends.isEmpty {
                Text("Add friends in the Friends tab to invite them.")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(.leading, 4)
            } else {
                VStack(alignment: .leading, spacing: 14) {
                    FlowLayout(spacing: 10) {
                        ForEach(model.people.friends) { friend in
                            choice(friend)
                        }
                    }
                    ForEach(model.people.friends.filter(\.identityChanged)) { friend in
                        ChangedIdentityNote(name: friend.personName)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .raised(Radius.xl)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func choice(_ friend: FriendEntry) -> some View {
        let selected = invited.contains(friend.userId)
        return Button {
            if selected {
                invited.remove(friend.userId)
            } else {
                invited.insert(friend.userId)
            }
        } label: {
            FriendChoice(name: friend.personName, key: friend.userId, selected: selected, changed: friend.identityChanged,
                         blocked: friend.identityChanged && !selected)
        }
        .buttonStyle(PressScaleStyle())
        .disabled(friend.identityChanged || creating)
        .accessibilityLabel(Text(friend.personName))
        .accessibilityValue(friend.identityChanged ? Text("New identity") : Text(verbatim: ""))
        .accessibilityHint(friend.identityChanged ? Text("Their identity changed. Trust them in Friends first.") : Text(verbatim: ""))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("rooms.create.friend.\(AccessibilityIdentifier.token(friend.userId))")
    }

    private func create() {
        guard valid, !creating else { return }
        creating = true
        failure = nil
        let friends = model.people.friends.map(\.userId).filter { invited.contains($0) }
        Task {
            defer { creating = false }
            do {
                let roomId = try await model.rooms.create(name: name, inviting: friends)
                made += 1
                created(roomId)
                dismiss()
            } catch is CancellationError {
                return
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}

/// Why a friend with the caution dot cannot be invited now.
private struct ChangedIdentityNote: View {
    @Environment(\.theme) private var theme
    let name: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Circle().fill(theme.colors.caution).frame(width: 6, height: 6)
                Text(name).appTextStyle(.caption).foregroundStyle(theme.colors.ink)
            }
            Text("Their identity changed. Trust them in Friends first.")
                .appTextStyle(.caption).fontWeight(.regular)
                .foregroundStyle(theme.colors.inkMuted)
                .padding(.leading, 12)
        }
        .accessibilityElement(children: .combine)
    }
}
