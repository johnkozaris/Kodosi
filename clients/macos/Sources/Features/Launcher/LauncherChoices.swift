import AppKit
import KodosiDesign
import SwiftUI

struct AccountChoices: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let info: AgentInfo
    let selected: String
    let choose: (String) -> Void
    @State private var adding = false
    @State private var name = ""
    @State private var saving = false
    @State private var error: String?
    @FocusState private var focused: Bool

    var body: some View {
        ChoicePanel(title: "Account") {
            ForEach(info.accounts) { account in
                ChoiceRow(
                    title: account.name,
                    detail: account.identity,
                    selected: account.command == selected,
                    identifier: "launcher.account.\(AccessibilityIdentifier.token(account.name))"
                ) { choose(account.command) }
            }
            if info.addsAccounts {
                if adding {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 8) {
                            TextField("Name, such as work", text: $name)
                                .textFieldStyle(WellTextFieldStyle()).focused($focused)
                                .onSubmit(add).accessibilityIdentifier("launcher.account.name")
                            Button("Add", action: add).buttonStyle(.kodosi(.primary, size: .small))
                                .disabled(saving || name.trimmingCharacters(in: .whitespaces).isEmpty)
                                .accessibilityIdentifier("launcher.account.add")
                        }
                        if let error {
                            ErrorNote(message: error)
                        }
                    }
                    .padding(.horizontal, 8).padding(.top, 4)
                    .onAppear { focused = true }
                } else {
                    ChoiceRow(title: String(localized: "Add an account"), symbol: "plus", identifier: "launcher.account.new") {
                        withAnimation(theme.motion.snappy) { adding = true }
                    }
                }
            }
        }
    }

    private func add() {
        let name = name.trimmingCharacters(in: .whitespaces)
        guard !saving, !name.isEmpty else { return }
        saving = true
        error = nil
        Task { @MainActor in
            defer { saving = false }
            do {
                let account = try await deps.launcher.addAccount(info.agent, name: name)
                choose(account.command)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

struct ModelChoices: View {
    @Environment(\.theme) private var theme
    let models: [AgentModel]?
    let loading: Bool
    let selected: String?
    let choose: (String?) -> Void
    @State private var filter = ""

    private var shown: [AgentModel] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return models ?? [] }
        return (models ?? []).filter { $0.label.localizedCaseInsensitiveContains(query) || $0.id.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        ChoicePanel(title: "Model") {
            if (models?.count ?? 0) > 8 {
                TextField("Find a model", text: $filter)
                    .textFieldStyle(WellTextFieldStyle())
                    .padding(.horizontal, 4).padding(.bottom, 2)
                    .accessibilityIdentifier("launcher.model.filter")
            }
            ScrollView {
                VStack(spacing: 0) {
                    ChoiceRow(title: String(localized: "Default"), detail: String(localized: "The agent's own choice"),
                              selected: selected == nil, identifier: "launcher.model.default") { choose(nil) }
                    if models == nil {
                        Text(loading ? "Asking the agent" : "The agent did not list its models")
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .shimmer(loading).padding(10)
                    }
                    ForEach(shown) { model in
                        ChoiceRow(
                            title: model.label,
                            detail: model.label.caseInsensitiveCompare(model.id) == .orderedSame ? nil : model.id, monoDetail: true,
                            selected: model.argument == selected,
                            identifier: "launcher.model.\(AccessibilityIdentifier.token(model.id))"
                        ) { choose(model.argument) }
                    }
                }
            }
            .frame(maxHeight: 340)
            .fixedSize(horizontal: false, vertical: (models?.count ?? 0) <= 8)
        }
    }
}

struct EffortChoices: View {
    let efforts: [AgentOption]
    let selected: String?
    let choose: (String?) -> Void

    var body: some View {
        ChoicePanel(title: "Effort") {
            ChoiceRow(title: String(localized: "Default"), detail: String(localized: "The model's own level"),
                      selected: selected == nil, identifier: "launcher.effort.default") { choose(nil) }
            ForEach(efforts) { effort in
                ChoiceRow(title: effort.label, selected: effort.argument == selected,
                          identifier: "launcher.effort.\(effort.id)") { choose(effort.argument) }
            }
        }
    }
}

struct FolderChoices: View {
    let recents: [String]
    let selected: String?
    let pick: (String) -> Void
    let choose: () -> Void

    var body: some View {
        ChoicePanel(title: "Folder") {
            ForEach(recents, id: \.self) { path in
                ChoiceRow(
                    title: URL(fileURLWithPath: path).lastPathComponent, detail: tilde(path),
                    monoDetail: true, selected: path == selected,
                    identifier: "launcher.folder.\(AccessibilityIdentifier.token(path))"
                ) { pick(path) }
            }
            ChoiceRow(title: String(localized: "Choose a folder…"), symbol: "folder", identifier: "launcher.folder.choose", action: choose)
        }
    }
}

/// `path` with `~` for the home folder.
func tilde(_ path: String) -> String {
    let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
    return path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}
