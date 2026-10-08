import SwiftUI

struct KodosiCommands: Commands {
    let dependencies: AppDependencies

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Session") { dependencies.newSession() }
                .keyboardShortcut("n")
            Button("History…") { dependencies.workbench.showsHistory = true }
                .keyboardShortcut("n", modifiers: [.command, .shift])
        }
        CommandMenu("Session") {
            Button("Session Details") { dependencies.workbench.detailsSessionId = dependencies.workbench.selectedSessionId }
                .keyboardShortcut("i")
                .disabled(dependencies.workbench.selectedSessionId == nil)
            Button("Focus Session") {
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
            Button("Toggle sidebar") { dependencies.workbench.sidebarCollapsed.toggle() }.keyboardShortcut("b")
        }
        CommandMenu("Navigate") {
            Button("Previous terminal") { dependencies.workbench.selectAdjacentSession(offset: -1) }.keyboardShortcut("[", modifiers: [.command, .shift])
            Button("Next terminal") { dependencies.workbench.selectAdjacentSession(offset: 1) }.keyboardShortcut("]", modifiers: [.command, .shift])
        }
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { dependencies.workbench.section = .settings }
                .keyboardShortcut(",")
        }
    }
}
