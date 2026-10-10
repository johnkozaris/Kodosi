import SwiftUI

struct NewBranchPopover: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let directory: String
    @State private var branch = generateSessionName().lowercased().replacingOccurrences(of: " ", with: "-")
    @State private var starting = false
    @State private var error: String?
    @FocusState private var focused: Bool

    private var name: String {
        branch.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("New branch", systemImage: "arrow.triangle.branch").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
            TextField("Branch name", text: $branch)
                .textFieldStyle(WellTextFieldStyle()).focused($focused)
                .onSubmit(start).accessibilityIdentifier("branch.name")
            Text("The branch gets its own folder, and a terminal starts there.")
                .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                .fixedSize(horizontal: false, vertical: true)
            if let error {
                ErrorNote(message: error)
            }
            Button(action: start) {
                Text(starting ? "Starting…" : "Start").frame(maxWidth: .infinity).shimmer(starting)
            }
            .buttonStyle(.kodosi(.primary)).disabled(starting || name.isEmpty)
            .accessibilityIdentifier("branch.start")
        }
        .padding(16)
        .popoverSheet(width: 280)
        .onAppear { focused = true }
    }

    private func start() {
        guard !starting, !name.isEmpty else { return }
        starting = true
        error = nil
        Task { @MainActor in
            defer { starting = false }
            do {
                try await deps.createSession(name: name, directory: directory, branch: name)
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}
