import AppKit

@MainActor
func pickWorkingDirectory(
    initialDirectory: String? = nil,
    message: String = "Select a working directory for the new session"
) -> String? {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.canCreateDirectories = true
    panel.allowsMultipleSelection = false
    panel.prompt = "Choose"
    panel.message = message
    panel.directoryURL = initialDirectory.map(URL.init(fileURLWithPath:))

    guard panel.runModal() == .OK, let url = panel.url else { return nil }
    return url.path
}
