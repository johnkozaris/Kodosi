import KodosiBridge
import KodosiDesign
import SwiftUI

struct RepositoryPicker: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.openURL) private var openURL
    let roomId: String
    var close: () -> Void = {}
    @State private var choices: RepositoryChoices?
    @State private var failure: String?
    @State private var query = ""
    @State private var adding: String?
    @FocusState private var searching: Bool

    private var linked: Set<String> {
        Set((deps.rooms[roomId]?.repositories ?? []).map { $0.url.lowercased() })
    }

    private var shown: [RepositoryChoice] {
        let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let all = choices?.repositories ?? []
        return words.isEmpty ? all : all.filter { $0.name.localizedCaseInsensitiveContains(words) }
    }

    private var address: String? {
        let text = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let looksLikeOne = text.contains("://") || text.hasPrefix("git@") || text.filter { $0 == "/" }.count >= 2
        return looksLikeOne && !text.contains(" ") ? text : nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").font(.system(size: 11, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                TextField("Find or paste a repository", text: $query)
                    .textFieldStyle(.plain).appTextStyle(.body).focused($searching)
                    .onSubmit(submit).onExitCommand(perform: close)
                    .accessibilityIdentifier("room.repository.find")
            }
            .padding(.horizontal, 12).frame(height: 34).wellCapsule()
            if let github = choices?.github, github != .ready {
                notice(github)
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 1) {
                    if let address {
                        row(RepositoryChoice(name: address, url: address, provider: ""), link: true)
                    }
                    ForEach(shown) { choice in row(choice) }
                    if choices == nil, failure == nil {
                        ForEach(0 ..< 3, id: \.self) { _ in
                            RoundedRectangle(cornerRadius: Radius.sm, style: .continuous).fill(theme.colors.well)
                                .frame(height: 34).shimmer(true).padding(.vertical, 3)
                        }
                    } else if shown.isEmpty, address == nil {
                        Text(query.isEmpty ? "No repositories found" : "No match")
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint).padding(10)
                    }
                }
            }
            .frame(maxHeight: 300).fixedSize(horizontal: false, vertical: true)
            if let failure {
                ErrorNote(message: failure)
            }
        }
        .animation(theme.motion.snappy, value: choices)
        .task {
            searching = true
            await load()
        }
    }

    private func notice(_ github: RepositoryChoices.GitHub) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "person.crop.circle.badge.plus").font(.system(size: 14, weight: .medium))
                .foregroundStyle(theme.colors.accentStrong)
            Text(github == .missing ? "Install GitHub CLI to see your repositories" : "Connect GitHub to see your repositories")
                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            Spacer(minLength: 6)
            if github == .missing {
                Button("Get it") { openURL(URL(string: "https://cli.github.com")!) }
                    .buttonStyle(.kodosi(.tinted, size: .small))
            } else {
                Button("Connect") {
                    deps.connectGitHub()
                    close()
                }
                .buttonStyle(.kodosi(.tinted, size: .small)).accessibilityIdentifier("room.github.connect")
            }
        }
        .padding(.horizontal, 10).frame(height: 40)
        .background(theme.colors.accentSoft.opacity(0.5), in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
    }

    private func row(_ choice: RepositoryChoice, link: Bool = false) -> some View {
        let url = choice.url
        let done = linked.contains(url.lowercased())
        let symbol = link ? "link" : choice.folder == nil ? "shippingbox.fill" : "folder.fill"
        return Button { add(url) } label: {
            HStack(spacing: 10) {
                IconTile(symbol: symbol, tint: choice.folder == nil || link ? TileTint.graphite : TileTint.amber, size: 24)
                VStack(alignment: .leading, spacing: 1) {
                    Text(choice.name).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(1)
                    if let folder = choice.folder {
                        Text(tilde(folder)).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                            .lineLimit(1).truncationMode(.middle)
                    }
                }
                Spacer(minLength: 6)
                if choice.private {
                    Image(systemName: "lock.fill").font(.system(size: 9)).foregroundStyle(theme.colors.inkFaint)
                }
                if adding == url {
                    ProgressView().controlSize(.small)
                } else if done {
                    Image(systemName: "checkmark").font(.system(size: 11, weight: .bold)).foregroundStyle(theme.colors.ready)
                }
            }
            .padding(.horizontal, 8).frame(height: choice.folder == nil ? 36 : 44).contentShape(Rectangle())
            .hoverRow()
        }
        .buttonStyle(.plain).disabled(done || adding != nil)
        .accessibilityIdentifier("room.repository.choice.\(AccessibilityIdentifier.token(url))")
    }

    private func submit() {
        if let address {
            add(address)
        } else if shown.count == 1, let only = shown.first {
            add(only.url)
        }
    }

    private func load() async {
        do {
            choices = try await deps.repositoryChoices(for: roomId)
        } catch { failure = error.localizedDescription }
    }

    private func add(_ url: String) {
        guard adding == nil else { return }
        adding = url
        failure = nil
        Task { @MainActor in
            defer { adding = nil }
            do {
                try await deps.roomAction(roomId, RoomAction.addRepository(url))
                query = ""
                close()
            } catch { failure = error.localizedDescription }
        }
    }
}

struct IssuePicker: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    @State private var query = ""
    @State private var loading: Set<String> = []
    @State private var failures: [String: String] = [:]
    @State private var adding: String?

    private var repositories: [RoomRepository] {
        deps.rooms[roomId]?.repositories ?? []
    }

    var body: some View {
        if repositories.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                Text("Link a repository").appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                RepositoryPicker(roomId: roomId)
            }
        } else {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass").font(.system(size: 11, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                    TextField("Find an issue", text: $query).textFieldStyle(.plain).appTextStyle(.body)
                        .accessibilityIdentifier("room.issue.find")
                }
                .padding(.horizontal, 12).frame(height: 34).wellCapsule()
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        ForEach(repositories) { repository in section(repository) }
                    }
                }
                .frame(maxHeight: 360).fixedSize(horizontal: false, vertical: true)
            }
            .task(id: repositories.map(\.id)) { await load() }
        }
    }

    @ViewBuilder
    private func section(_ repository: RoomRepository) -> some View {
        let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let issues = (deps.roomIssues[repository.id] ?? []).filter {
            !$0.closed && (words.isEmpty || $0.title.localizedCaseInsensitiveContains(words) || "#\($0.number)".contains(words))
        }
        let number = Self.number(in: words).flatMap { number in issues.contains { $0.number == number } ? nil : number }
        HStack(spacing: 6) {
            Image(systemName: "shippingbox").font(.system(size: 10, weight: .semibold))
            Text(repository.name).appTextStyle(.caption)
            if loading.contains(repository.id) {
                ProgressView().controlSize(.mini)
            }
        }
        .foregroundStyle(theme.colors.inkFaint).padding(.horizontal, 8).padding(.top, 8).padding(.bottom, 2)
        if let failure = failures[repository.id] {
            Text(failure).appTextStyle(.caption).foregroundStyle(theme.colors.danger).padding(.horizontal, 8)
        } else if issues.isEmpty, number == nil, !loading.contains(repository.id) {
            Text(words.isEmpty ? "No open issues" : "No match").appTextStyle(.caption)
                .foregroundStyle(theme.colors.inkFaint).padding(.horizontal, 8).padding(.bottom, 4)
        }
        ForEach(issues) { issue in row(issue, in: repository) }
        if let number {
            numberRow(number, in: repository)
        }
    }

    static func number(in text: String) -> UInt64? {
        let digits = text.hasPrefix("#") ? text.dropFirst() : Substring(text)
        return digits.allSatisfy { $0.isASCII && $0.isWholeNumber } ? UInt64(digits).flatMap { $0 > 0 ? $0 : nil } : nil
    }

    private func numberRow(_ number: UInt64, in repository: RoomRepository) -> some View {
        Button { add(number, from: repository) } label: {
            HStack(spacing: 10) {
                Image(systemName: "number").font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.inkMuted)
                Text("Add issue #\(String(number))").appTextStyle(.body).foregroundStyle(theme.colors.ink)
                Spacer(minLength: 6)
                if adding == key(number, in: repository) {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: "plus").font(.system(size: 11, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
                }
            }
            .padding(.horizontal, 8).frame(height: 34).contentShape(Rectangle()).hoverRow()
        }
        .buttonStyle(.plain).disabled(adding != nil)
        .accessibilityIdentifier("room.issue.number.\(number)")
    }

    private func row(_ issue: RoomIssue, in repository: RoomRepository) -> some View {
        let imported = deps.rooms[roomId]?.tasks.contains { $0.issue?.url == issue.url } == true
        return Button { add(issue.number, from: repository) } label: {
            HStack(spacing: 10) {
                Image(systemName: "smallcircle.filled.circle").font(.system(size: 13)).foregroundStyle(theme.colors.ready)
                Text(issue.title).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                Spacer(minLength: 6)
                Text(verbatim: "#\(issue.number)").appTextStyle(.caption).monospacedDigit().foregroundStyle(theme.colors.inkFaint)
                if adding == key(issue.number, in: repository) {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: imported ? "checkmark" : "plus").font(.system(size: 11, weight: .bold))
                        .foregroundStyle(imported ? theme.colors.ready : theme.colors.inkMuted)
                        .contentTransition(.symbolEffect(.replace))
                }
            }
            .padding(.horizontal, 8).frame(height: 34).contentShape(Rectangle()).hoverRow()
        }
        .buttonStyle(.plain).disabled(imported || adding != nil)
        .accessibilityLabel(imported ? Text("\(issue.title) is a task") : Text("Add \(issue.title) to tasks"))
    }

    private func load() async {
        for repository in repositories {
            loading.insert(repository.id)
            failures[repository.id] = nil
            do {
                try await deps.roomAction(roomId, RoomAction.issues(repository.id))
            } catch { failures[repository.id] = error.localizedDescription }
            loading.remove(repository.id)
        }
    }

    private func key(_ number: UInt64, in repository: RoomRepository) -> String {
        "\(repository.id)#\(number)"
    }

    private func add(_ number: UInt64, from repository: RoomRepository) {
        adding = key(number, in: repository)
        Task { @MainActor in
            defer { adding = nil }
            do {
                try await deps.roomAction(roomId, RoomAction.importIssue(repositoryId: repository.id, number: number))
                query = ""
            } catch { failures[repository.id] = error.localizedDescription }
        }
    }
}
