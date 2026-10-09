import SwiftUI

struct StartCommandsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        @Bindable var settings = deps.settings
        ListGroup(title: "Start", footer: "A terminal at its prompt shows these marks. Select a mark to start its command there.") {
            ForEach($settings.startCommands) { $command in
                HStack(spacing: 12) {
                    StartMark(command: command, repeated: settings.startCommands.first { $0.agent == command.agent } != command, size: 22) {}
                        .allowsHitTesting(false).frame(width: 28)
                    TextField("Name", text: $command.name)
                        .textFieldStyle(.plain).appTextStyle(.body).frame(width: 150)
                        .accessibilityIdentifier("settings.start.\(AccessibilityIdentifier.token(command.id)).name")
                    TextField("Command", text: $command.command)
                        .textFieldStyle(.plain).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted)
                        .accessibilityIdentifier("settings.start.\(AccessibilityIdentifier.token(command.id)).command")
                    IconButton(title: "Remove", symbol: "xmark", identifier: "settings.start.\(AccessibilityIdentifier.token(command.id)).remove",
                               size: 24, destructive: true)
                    {
                        settings.startCommands.removeAll { $0.id == command.id }
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 46)
            }
            Button {
                settings.startCommands.append(StartCommand(name: "", command: ""))
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: "plus").font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.accentStrong).frame(width: 28)
                    Text("Add a command").appTextStyle(.body).foregroundStyle(theme.colors.accentStrong)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 14).frame(minHeight: 46).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("settings.start.add")
        }
        .animation(theme.motion.spring, value: settings.startCommands.map(\.id))
        .onChange(of: settings.startCommands) { _, _ in settings.saveStartCommands() }
    }
}
