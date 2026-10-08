import SwiftUI

struct KodosiCommands: Commands {
    let dependencies: AppDependencies

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Terminal") { dependencies.newSession() }
                .keyboardShortcut("n")
            Button("Resume a Conversation…") { dependencies.workbench.showsHistory = true }
                .keyboardShortcut("n", modifiers: [.command, .shift])
            Button("Go to…") { dependencies.workbench.showsPalette.toggle() }
                .keyboardShortcut("k")
        }
        CommandMenu("Terminal") {
            Button("Details") { dependencies.workbench.detailsSessionId = dependencies.workbench.selectedSessionId }
                .keyboardShortcut("i")
                .disabled(dependencies.workbench.selectedSessionId == nil)
            Button("Zoom") {
                if let id = dependencies.workbench.selectedSessionId {
                    dependencies.workbench.toggleFocus(id)
                }
            }
            .keyboardShortcut("f", modifiers: [.command, .shift])
            .disabled(dependencies.workbench.selectedSessionId == nil)
            Button("Minimize") {
                if let id = dependencies.workbench.selectedSessionId {
                    dependencies.dismissSession(id)
                }
            }
            .keyboardShortcut("w", modifiers: [.command, .shift])
            .disabled(dependencies.workbench.selectedSessionId == nil)
        }
        CommandGroup(after: .sidebar) {
            Button("Show or Hide Sidebar") { dependencies.workbench.sidebarCollapsed.toggle() }.keyboardShortcut("b")
        }
        CommandMenu("Navigate") {
            Button("Previous Terminal") { dependencies.workbench.selectAdjacentSession(offset: -1) }.keyboardShortcut("[", modifiers: [.command, .shift])
            Button("Next Terminal") { dependencies.workbench.selectAdjacentSession(offset: 1) }.keyboardShortcut("]", modifiers: [.command, .shift])
        }
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { dependencies.workbench.section = .settings }
                .keyboardShortcut(",")
        }
    }
}
