import Foundation
import KodosiBridge
import Observation

struct NewTerminalRequest: Identifiable {
    let id = UUID()
    var computer: String?
    var folder: String?
    var projectId: String?
}

@Observable
final class NewTerminal {
    static let lastComputerKey = "terminals.lastComputer"

    var computer: String?
    var prompt = ""
    let projectId: String?
    private(set) var folders: [String: String] = [:]
    private(set) var starting = false
    private(set) var failure: String?
    @ObservationIgnored private let defaults: UserDefaults

    init(request: NewTerminalRequest, online: [String], defaults: UserDefaults = .standard) {
        projectId = request.projectId
        self.defaults = defaults
        computer = Self.computer(section: request.computer, last: defaults.string(forKey: Self.lastComputerKey), online: online)
        if let section = request.computer, let folder = request.folder {
            folders[section] = folder
        }
    }

    static func computer(section: String?, last: String?, online: [String]) -> String? {
        [section, last].compactMap(\.self).first { online.contains($0) } ?? online.first
    }

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

    func start(in launcher: LauncherState, on terminals: TerminalStore) async -> String? {
        guard !starting, let computer = launcher.computer, ready(in: launcher) else { return nil }
        let agent = agent(in: launcher).flatMap { LaunchPlan(launcher: launcher, agent: $0.agent).start }
        let words = String(prompt.map { $0.isNewline || $0 == "\t" ? " " : $0 }).trimmingCharacters(in: .whitespaces)
        let terminal = TerminalStart(
            name: generateSessionName(), directory: folder(in: launcher), computer: computer, projectId: projectId, agent: agent,
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
