import KodosiBridge
import KodosiDesign
import SwiftUI

struct RenameSheet: View {
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @FocusState private var focused: Bool
    let title: LocalizedStringKey
    let field: LocalizedStringKey
    let identifier: String
    let rename: (String) -> Void

    init(title: LocalizedStringKey, field: LocalizedStringKey, name: String, identifier: String, rename: @escaping (String) -> Void) {
        _name = State(initialValue: name)
        self.title = title
        self.field = field
        self.identifier = identifier
        self.rename = rename
    }

    private var valid: Bool {
        ProductInput.validName(name)
    }

    var body: some View {
        NavigationStack {
            TextField(field, text: $name)
                .focused($focused)
                .submitLabel(.done)
                .onSubmit(save)
                .accessibilityIdentifier("\(identifier).field")
                .padding(.horizontal, 20)
                .padding(.top, 4)
                .frame(maxWidth: 560, maxHeight: .infinity, alignment: .top)
                .frame(maxWidth: .infinity)
                .background(theme.colors.surface)
                .navigationTitle(title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { dismiss() }
                            .buttonStyle(.kodosi(.ghost, size: .small))
                            .fixedSize()
                            .accessibilityIdentifier("\(identifier).cancel")
                    }
                    .sharedBackgroundVisibility(.hidden)
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Rename", action: save)
                            .buttonStyle(.kodosi(.primary, size: .small))
                            .fixedSize()
                            .disabled(!valid)
                            .accessibilityIdentifier("\(identifier).save")
                    }
                    .sharedBackgroundVisibility(.hidden)
                }
        }
        .presentationDetents([.height(132)])
        .kodosiSheet()
        .onAppear { focused = true }
    }

    private func save() {
        guard valid else { return }
        rename(name)
        dismiss()
    }
}
