import KodosiBridge
import SwiftUI

struct KodosiCommands: Commands {
    let dependencies: AppDependencies

    var body: some Commands {
        CommandGroup(after: .appInfo) {
            Button("Check for Updates…") { dependencies.updater.check() }
                .disabled(!dependencies.updater.configured)
        }
        CommandGroup(replacing: .newItem) {
            item("New Terminal", .newTerminal)
            item("Quick Terminal", .quickTerminal)
            item("Resume a Conversation…", .resume)
            item("Go to…", .goTo)
        }
        CommandGroup(replacing: .saveItem) {
            item("Close Terminal", .close)
        }
        CommandMenu("Terminal") {
            item("Details", .details)
            item("Zoom", .zoom)
            item("Minimize", .minimize)
        }
        CommandGroup(after: .sidebar) {
            item("Show or Hide Sidebar", .sidebar)
            item("Dosi", .dosi)
            item("Stop Dosi", .stopDosi)
        }
        CommandMenu("Navigate") {
            item("Previous Terminal", .previousTerminal)
            item("Next Terminal", .nextTerminal)
            item("Terminal on the Left", .leftTerminal)
            item("Terminal on the Right", .rightTerminal)
            item("Terminal Above", .upTerminal)
            item("Terminal Below", .downTerminal)
        }
        CommandGroup(replacing: .appSettings) {
            item("Settings…", .settings)
        }
    }

    private func item(_ title: LocalizedStringKey, _ action: AppShortcut) -> some View {
        Button(title) { dependencies.perform(action) }
            .keyboardShortcut(dependencies.settings.shortcuts.keys(for: action).first?.keyboardShortcut)
            .disabled(!dependencies.canPerform(action))
    }
}
