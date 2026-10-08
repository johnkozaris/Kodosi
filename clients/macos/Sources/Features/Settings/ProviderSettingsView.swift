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
        VStack(alignment: .leading, spacing: 20) {
            Text("Providers").appTextStyle(.headingSection)
            KodosiPicker("Provider", selection: $provider, values: Provider.allCases, label: { $0.name })
            HStack {
                Text(directory ?? String(localized: "User configuration")).appTextStyle(.caption).lineLimit(1).truncationMode(.middle)
                Spacer()
                Button("Choose Project…") {
                    if let selected = pickWorkingDirectory(initialDirectory: directory) {
                        directory = selected
                    }
                }
                if directory != nil {
                    Button("Clear Project") { directory = nil }.accessibilityIdentifier("settings.providers.clearProject")
                }
            }
            if loading {
                ProgressView("Checking provider…")
            }
            if let configuration {
                if let executable = configuration.executable {
                    Text(executable).appTextStyle(.monoCaption).textSelection(.enabled)
                } else {
                    Text("The provider executable was not found.").appTextStyle(.body)
                }
                ForEach(configuration.files) { file in
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(file.label).appTextStyle(.body)
                            Text(file.path).appTextStyle(.monoCaption).foregroundStyle(theme.colors.mutedForeground).textSelection(.enabled)
                        }
                        Spacer()
                        if file.exists {
                            Button(file.editable ? String(localized: "Open") : String(localized: "View")) {
                                do {
                                    try NativeFiles.open(file.path)

                                } catch {
                                    message = error.localizedDescription
                                }
                            }
                        } else {
                            Text("Not created").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                        }
                    }
                }
                if let notice = configuration.message {
                    Text(notice).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                }
            }
            if let message {
                Text(message).appTextStyle(.caption).foregroundStyle(theme.colors.destructive)
            }
        }
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
