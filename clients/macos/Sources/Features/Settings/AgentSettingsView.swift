import KodosiBridge
import KodosiDesign
import SwiftUI

struct AgentSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var chosen: String?
    @State private var configuration: ProviderConfiguration?
    @State private var directory: String?
    @State private var loading = false
    @State private var message: String?

    private var launcher: LauncherState {
        deps.launcher
    }

    private var agents: [String] {
        guard let catalog = launcher.catalog else { return Provider.allCases.map(\.rawValue) }
        return catalog.agents.filter { $0.installed || Provider(rawValue: $0.agent) != nil }.map(\.agent)
    }

    private var agent: String {
        let agents = agents
        if let chosen, agents.contains(chosen) {
            return chosen
        }
        let installed = launcher.installed.map(\.agent)
        return launcher.choice.agent.flatMap { installed.contains($0) ? $0 : nil } ?? installed.first ?? agents.first
            ?? Provider.claude.rawValue
    }

    private var info: AgentInfo? {
        launcher.agent(agent)
    }

    private var provider: Provider? {
        Provider(rawValue: agent)
    }

    private var kind: AgentKind {
        AgentKind(program: agent)
    }

    private var activity: AgentMark.Activity {
        guard provider != nil else { return info?.installed == true ? .awake : .asleep }
        return loading ? .working : configuration?.executable == nil ? .asleep : .awake
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 14) {
                AgentMark(kind: kind, size: 52, activity: activity)
                VStack(alignment: .leading, spacing: 4) {
                    Text(kind.label).appTextStyle(.title).foregroundStyle(theme.colors.ink)
                    if loading {
                        Text("Looking on this Mac").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).shimmer()
                    } else if let executable = configuration?.executable {
                        HStack(spacing: 6) {
                            Circle().fill(theme.colors.ready).frame(width: 6, height: 6)
                            Text(executable).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted)
                                .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                        }
                    } else if configuration != nil {
                        Tag(text: String(localized: "Not installed on this Mac"), tone: .caution)
                    }
                }
                Spacer()
                SegmentedPill(selection: Binding(get: { agent }, set: { chosen = $0 }), options: agents.map {
                    SegmentOption(value: $0, title: AgentKind(program: $0).label)
                }, compact: true, identifier: "settings.agents")
            }
            if let info, info.installed {
                AgentDefaults(info: info)
            }
            if provider != nil {
                files
            }
            if let notice = configuration?.message {
                Text(notice).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
            if let message {
                ErrorNote(message: message)
            }
        }
        .animation(theme.motion.spring, value: configuration?.files.map(\.id))
        .task { await launcher.refresh() }
        .task(id: "\(agent):\(directory ?? ""):\(deps.accountEpoch)") { await inspect() }
    }

    private var files: some View {
        ListGroup(footer: "Kodosi opens these files. It does not change them.") {
            ListRow(directory.map(HostPath.folderName) ?? String(localized: "All folders"),
                    subtitle: directory, monoSubtitle: true, symbol: "folder.fill", tint: TileTint.amber)
            {
                if directory != nil {
                    Button("All folders") { directory = nil }.buttonStyle(.kodosi(.ghost, size: .small))
                        .accessibilityIdentifier("settings.agents.clearProject")
                }
                Button("Choose a folder…") {
                    Task { @MainActor in
                        if let selected = await pickWorkingDirectory(initialDirectory: directory) {
                            directory = selected
                        }
                    }
                }.buttonStyle(.kodosi(.secondary, size: .small))
            }
            if let configuration {
                ForEach(configuration.files) { file in
                    ListRow(file.label, subtitle: file.path, monoSubtitle: true, symbol: "doc.text.fill",
                            tint: file.exists ? TileTint.blue : TileTint.graphite)
                    {
                        if file.exists {
                            Button(file.editable ? String(localized: "Open") : String(localized: "View")) {
                                do {
                                    try NativeFiles.open(file.path)
                                } catch {
                                    message = error.localizedDescription
                                }
                            }.buttonStyle(.kodosi(.secondary, size: .small))
                        } else {
                            Tag(text: String(localized: "Not made yet"))
                        }
                    }
                }
            }
        }
    }

    private func inspect() async {
        configuration = nil
        message = nil
        loading = provider != nil
        guard let provider else { return }
        defer {
            if !Task.isCancelled {
                loading = false
            }
        }
        var fields: [String: JSONValue] = ["provider": .string(provider.rawValue)]
        if let directory {
            fields["workingDirectory"] = .string(directory)
        }
        do {
            let reply = try await deps.commandSink.request("provider.inspect", fields)
            guard !Task.isCancelled else { return }
            configuration = try reply.value("result")
        } catch {
            if !Task.isCancelled {
                message = error.localizedDescription
            }
        }
    }
}
