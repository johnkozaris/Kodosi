import Foundation
import KodosiBridge
import Observation
import os

/// The agents of this Mac and the models of each, as the runtime finds them.
@MainActor
@Observable
final class LauncherState {
    private let commands: CommandSink
    private(set) var catalog: AgentCatalog?
    private(set) var models: [String: AgentModels] = [:]
    private(set) var loadingModels: Set<String> = []
    private(set) var refreshing = false
    private var refreshedAt: Date?

    init(commands: CommandSink) {
        self.commands = commands
    }

    func agent(_ id: String?) -> AgentInfo? {
        catalog?.agents.first { $0.agent == id }
    }

    /// Asks the runtime again, unless it answered in the last few seconds.
    func refresh(force: Bool = false) async {
        guard !refreshing, force || refreshedAt.map({ Date().timeIntervalSince($0) > 8 }) ?? true else { return }
        refreshing = true
        defer { refreshing = false }
        do {
            let reply = try await commands.request("provider.agents")
            let catalog: AgentCatalog = try reply.value("result")
            if catalog != self.catalog {
                let installed = Set(catalog.agents.filter(\.installed).map(\.agent))
                models = models.filter { installed.contains($0.key) }
                self.catalog = catalog
            }
            refreshedAt = Date()
        } catch {
            Logger.app.warning("Agents of this Mac unknown: \(error.localizedDescription, privacy: .public)")
        }
    }

    func loadModels(_ agent: String) async {
        guard models[agent] == nil, !loadingModels.contains(agent), self.agent(agent)?.installed == true else { return }
        loadingModels.insert(agent)
        defer { loadingModels.remove(agent) }
        do {
            let reply = try await commands.request("provider.agentModels", ["agent": .string(agent)])
            models[agent] = try reply.value("result")
        } catch {
            Logger.app.warning("Models of \(agent, privacy: .public) unknown: \(error.localizedDescription, privacy: .public)")
        }
    }

    func addAccount(_ agent: String, name: String) async throws -> AgentAccount {
        let reply = try await commands.request("provider.addAccount", ["agent": .string(agent), "name": .string(name)])
        let account: AgentAccount = try reply.value("result")
        await refresh(force: true)
        return account
    }
}
