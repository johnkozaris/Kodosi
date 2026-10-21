import AppKit

@MainActor
func pickWorkingDirectory(
    initialDirectory: String? = nil,
    message: String = String(localized: "Choose a folder for the new terminal"),
) -> String? {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.canCreateDirectories = true
    panel.allowsMultipleSelection = false
    panel.prompt = String(localized: "Choose")
    panel.message = message
    panel.directoryURL = initialDirectory.map(URL.init(fileURLWithPath:))

    guard panel.runModal() == .OK, let url = panel.url else { return nil }
    return url.path
}
