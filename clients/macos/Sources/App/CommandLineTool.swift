import Foundation

/// The `kodosi` command for terminals outside Kodosi: a link to the app executable.
enum CommandLineTool {
    static let link = URL(fileURLWithPath: "/usr/local/bin/kodosi")
    private static let homebrewLink = URL(fileURLWithPath: "/opt/homebrew/bin/kodosi")

    static var installed: Bool {
        guard let own = Bundle.main.executableURL?.resolvingSymlinksInPath() else { return false }
        return [link, homebrewLink].contains { $0.resolvingSymlinksInPath() == own }
    }

    enum Outcome { case installed, cancelled, failed }

    /// Points `/usr/local/bin/kodosi` to this app. macOS asks for an administrator password.
    static func install() async -> Outcome {
        guard let own = Bundle.main.executablePath else { return .failed }
        let quoted = own.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"")
        let script = """
        do shell script "/bin/mkdir -p /usr/local/bin && /bin/ln -sf " & quoted form of "\(quoted)" \
        & " \(link.path)" with administrator privileges
        """
        return await Task.detached {
            let process = Process()
            let errors = Pipe()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
            process.arguments = ["-e", script]
            process.standardError = errors
            do { try process.run() } catch { return Outcome.failed }
            let message = String(bytes: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            process.waitUntilExit()
            if process.terminationStatus == 0 {
                return Outcome.installed
            }
            return message.contains("-128") ? Outcome.cancelled : Outcome.failed
        }.value
    }
}
