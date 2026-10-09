import SwiftUI

struct ProviderSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var provider: Provider = .claude
    @State private var configuration: ProviderConfiguration?
    @State private var directory: String?
    @State private var loading = false
    @State private var message: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 14) {
                AgentMark(kind: provider.agent, size: 52, activity: loading ? .working : configuration?.executable == nil ? .asleep : .awake)
                VStack(alignment: .leading, spacing: 4) {
                    Text(provider.agent.label).appTextStyle(.title).foregroundStyle(theme.colors.ink)
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
                SegmentedPill(selection: $provider, options: Provider.allCases.map {
                    SegmentOption(value: $0, title: $0.name)
                }, compact: true, identifier: "settings.providers")
            }
            ListGroup(footer: "Kodosi opens these files. It does not change them.") {
                ListRow(directory.map { URL(fileURLWithPath: $0).lastPathComponent } ?? String(localized: "All projects"),
                        subtitle: directory, monoSubtitle: true, symbol: "folder.fill", tint: TileTint.amber)
                {
                    if directory != nil {
                        Button("All projects") { directory = nil }.buttonStyle(.kodosi(.ghost, size: .small))
                            .accessibilityIdentifier("settings.providers.clearProject")
                    }
                    Button("Choose a project…") {
                        if let selected = pickWorkingDirectory(initialDirectory: directory) {
                            directory = selected
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
            if let notice = configuration?.message {
                Text(notice).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
            if let message {
                ErrorNote(message: message)
            }
        }
        .animation(theme.motion.spring, value: configuration?.files.map(\.id))
        .task(id: "\(provider.rawValue):\(directory ?? ""):\(deps.accountEpoch)") {
            configuration = nil; message = nil; loading = true
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
}

extension Provider {
    var agent: AgentKind {
        self == .claude ? .claude : .copilot
    }
}
