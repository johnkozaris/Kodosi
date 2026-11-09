import Foundation
import KodosiBridge
import Observation

/// A new terminal to start from this device: on a computer of the list, or in a room.
struct NewTerminalRequest: Identifiable {
    let id = UUID()
    var computer: String?
    var roomId: String?
}

/// The choices of a new terminal on one of the person's computers, and its start there.
@Observable
final class NewTerminal {
    static let lastComputerKey = "terminals.lastComputer"

    var computer: String?
    var prompt = ""
    let roomId: String?
    private(set) var folders: [String: String] = [:]
    private(set) var starting = false
    private(set) var failure: String?
    @ObservationIgnored private let defaults: UserDefaults

    init(request: NewTerminalRequest, online: [String], defaults: UserDefaults = .standard) {
        roomId = request.roomId
        self.defaults = defaults
        computer = Self.computer(section: request.computer, last: defaults.string(forKey: Self.lastComputerKey), online: online)
    }

    /// The computer of the list's section, else the one that started a terminal last, else the
    /// first one online.
    static func computer(section: String?, last: String?, online: [String]) -> String? {
        [section, last].compactMap(\.self).first { online.contains($0) } ?? online.first
    }

    /// The agent that the computer's last choice names, when that computer has it.
    func agent(in launcher: LauncherState) -> AgentInfo? {
        launcher.choice.agent.flatMap { agent in launcher.installed.first { $0.agent == agent } }
    }

    func folder(in launcher: LauncherState) -> String? {
        launcher.computer.flatMap { folders[$0] } ?? launcher.places?.recent.first
    }

    func choose(folder: String, in launcher: LauncherState) {
        guard let computer = launcher.computer else { return }
        folders[computer] = folder
    }

    func ready(in launcher: LauncherState) -> Bool {
        agent(in: launcher).map { LaunchPlan(launcher: launcher, agent: $0.agent).start != nil } ?? true
    }

    /// Starts the terminal on the launcher's computer and gives its identifier, or keeps the
    /// computer's words when it does not start.
    func start(in launcher: LauncherState, on terminals: TerminalStore) async -> String? {
        guard !starting, let computer = launcher.computer, ready(in: launcher) else { return nil }
        let agent = agent(in: launcher).flatMap { LaunchPlan(launcher: launcher, agent: $0.agent).start }
        let words = String(prompt.map { $0.isNewline || $0 == "\t" ? " " : $0 }).trimmingCharacters(in: .whitespaces)
        let terminal = TerminalStart(
            name: generateSessionName(), directory: folder(in: launcher), computer: computer, roomId: roomId, agent: agent,
            prompt: agent == nil || words.isEmpty ? nil : words,
        )
        starting = true
        failure = nil
        defer { starting = false }
        do {
            let started = try await terminals.start(terminal)
            defaults.set(computer, forKey: Self.lastComputerKey)
            return started
        } catch is CancellationError {
            return nil
        } catch {
            failure = error.localizedDescription
            Task { await launcher.refresh(force: true) }
            return nil
        }
    }
}
