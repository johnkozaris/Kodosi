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
            List {
                Section {
                    VStack(spacing: 18) {
                        RoomSigil(name: name, key: name.isEmpty ? "new" : name, size: 68)
                            .animation(theme.motion.spring, value: name)
                        TextField("Launch week", text: $name)
                            .textFieldStyle(WellTextFieldStyle(radius: Radius.lg))
                            .focused($focused)
                            .submitLabel(.done)
                            .onSubmit(create)
                            .accessibilityLabel(Text("Room name"))
                            .accessibilityIdentifier("rooms.create.name")
                    }
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                } footer: {
                    Text("Everyone in the room shares its terminals and conversation.")
                }
                friendsSection
                if let failure {
                    Section {
                        ErrorNote(message: failure)
                            .listRowInsets(EdgeInsets())
                            .listRowBackground(Color.clear)
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(theme.colors.surface)
            .navigationTitle("New room")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .accessibilityIdentifier("rooms.create.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(action: create) {
                        Text(creating ? "Making…" : "Make room")
                    }
                    .disabled(!valid || creating)
                    .accessibilityIdentifier("rooms.create.confirm")
                }
            }
            .animation(theme.motion.spring, value: failure)
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
        .interactiveDismissDisabled(creating)
        .sensoryFeedback(.success, trigger: made)
        .onAppear { focused = true }
    }

    @ViewBuilder
    private var friendsSection: some View {
        let friends = model.people.friends
        Section {
            if friends.isEmpty {
                Text("Add friends in People to invite them.")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .listRowBackground(theme.colors.raised)
            }
            ForEach(friends) { friend in
                friendRow(friend)
            }
        } header: {
            Text("Invite friends")
        }
    }

    private func friendRow(_ friend: FriendEntry) -> some View {
        let selected = invited.contains(friend.userId)
        let name = friend.personName
        return Button {
            if selected {
                invited.remove(friend.userId)
            } else {
                invited.insert(friend.userId)
            }
        } label: {
            HStack(spacing: 12) {
                PersonAvatar(name: name, key: friend.userId, size: 34)
                VStack(alignment: .leading, spacing: 1) {
                    Text(name)
                        .appTextStyle(.body)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(1)
                    if friend.identityChanged {
                        Text("Their identity changed. Trust them in People first.")
                            .appTextStyle(.caption)
                            .foregroundStyle(theme.colors.caution)
                    } else {
                        Text(verbatim: "@\(friend.handle)")
                            .appTextStyle(.caption)
                            .foregroundStyle(theme.colors.inkFaint)
                    }
                }
                Spacer(minLength: 8)
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 22))
                    .foregroundStyle(selected ? theme.colors.accent : theme.colors.inkFaint)
                    .contentTransition(.symbolEffect(.replace))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(friend.identityChanged || creating)
        .listRowBackground(theme.colors.raised)
        .sensoryFeedback(.selection, trigger: selected)
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
