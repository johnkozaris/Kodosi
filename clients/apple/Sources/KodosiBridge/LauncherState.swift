import Foundation
import Observation
import os

@MainActor
@Observable
public final class LauncherState {
    private let commands: CommandSink
    public let computer: String?
    public private(set) var catalog: AgentCatalog?
    public private(set) var choice = AgentChoice()
    public private(set) var models: [String: AgentModels] = [:]
    public private(set) var loadingModels: Set<String> = []
    public private(set) var places: FolderList?
    public private(set) var usage: [String: AccountUsage] = [:]
    public private(set) var usageRead = false
    public private(set) var failure: String?
    public private(set) var readingUsage = false
    private var refreshing = false
    private var refreshesAgain = false
    private var refreshedAt: Date?
    private var changes = 0

    private struct Chosen: Decodable {
        var choice: AgentChoice
    }

    public init(commands: CommandSink, computer: String? = nil) {
        self.commands = commands
        self.computer = computer
    }

    public var installed: [AgentInfo] {
        catalog?.agents.filter(\.installed) ?? []
    }

    public func agent(_ id: String?) -> AgentInfo? {
        catalog?.agents.first { $0.agent == id }
    }

    public func refresh(force: Bool = false) async {
        if refreshing {
            refreshesAgain = refreshesAgain || force
            return
        }
        guard force || refreshedAt.map({ Date().timeIntervalSince($0) > 8 }) ?? true else { return }
        refreshing = true
        defer { refreshing = false }
        repeat {
            refreshesAgain = false
            failure = nil
            do {
                let asOf = changes
                let reply = try await commands.request("provider.agents", fields())
                let catalog: AgentCatalog = try reply.value("result")
                if catalog != self.catalog {
                    let installed = Set(catalog.agents.filter(\.installed).map(\.agent))
                    models = models.filter { installed.contains($0.key) }
                    self.catalog = catalog
                }
                try adopt(reply.value("result", as: Chosen.self).choice, asOf: asOf)
                refreshedAt = Date()
            } catch {
                if !(error is CancellationError) {
                    failure = error.localizedDescription
                }
                Logger.app.warning("Agents of a computer unknown: \(error.localizedDescription, privacy: .public)")
            }
        } while refreshesAgain
    }

    public var usageAccounts: [(info: AgentInfo, account: AgentAccount)] {
        installed.filter(\.reportsUsage).flatMap { info in
            info.accounts.filter { $0.identity != nil }.map { (info, $0) }
        }
    }

    public func usage(of agent: String, account: String) -> AccountUsage? {
        usage[AccountUsage.key(agent, account)]
    }

    /// The host reads plan usage through each provider's command, which takes seconds the first
    /// time.
    public var loadingUsage: Bool {
        readingUsage && !usageRead
    }

    public func refreshUsage() async {
        guard !readingUsage else { return }
        readingUsage = true
        defer { readingUsage = false }
        do {
            let reply = try await commands.request("provider.usage", fields())
            let answer: AgentUsageReply = try reply.value("result")
            let read = Dictionary(answer.accounts.map { (AccountUsage.key($0.agent, $0.account), $0) }) { first, _ in first }
            if read != usage {
                usage = read
            }
            usageRead = true
        } catch {
            Logger.app.warning("Plan usage unknown: \(error.localizedDescription, privacy: .public)")
        }
    }

    public func loadModels(_ agent: String) async {
        guard models[agent] == nil, !loadingModels.contains(agent), self.agent(agent)?.installed == true else { return }
        loadingModels.insert(agent)
        defer { loadingModels.remove(agent) }
        do {
            let asOf = changes
            let reply = try await commands.request("provider.agentModels", fields(["agent": .string(agent)]))
            models[agent] = try reply.value("result")
            try adopt(reply.value("result", as: Chosen.self).choice, asOf: asOf)
        } catch {
            Logger.app.warning("Models of \(agent, privacy: .public) unknown: \(error.localizedDescription, privacy: .public)")
        }
    }

    public func choose(_ change: AgentChoiceChange) {
        changes += 1
        let asOf = changes
        choice.apply(change)
        Task {
            do {
                let reply = try await commands.request("provider.choose", fields(["change": change.json]))
                try adopt(reply.value("result"), asOf: asOf)
            } catch {
                Logger.app.warning("Launch choice not kept: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    public func addAccount(_ agent: String, name: String) async throws -> AgentAccount {
        let reply = try await commands.request("provider.addAccount", fields(["agent": .string(agent), "name": .string(name)]))
        let account: AgentAccount = try reply.value("result")
        if let index = catalog?.agents.firstIndex(where: { $0.agent == agent }) {
            catalog?.agents[index].accounts.append(account)
        }
        await refresh(force: true)
        return account
    }

    public func folders(at path: String? = nil) async throws -> FolderList {
        let reply = try await commands.request("folders.list", fields(path.map { ["path": .string($0)] } ?? [:]))
        let list: FolderList = try reply.value("folders")
        if path == nil {
            places = list
        }
        return list
    }

    private func fields(_ fields: [String: JSONValue] = [:]) -> [String: JSONValue] {
        var fields = fields
        if let computer {
            fields["computer"] = .string(computer)
        }
        return fields
    }

    private func adopt(_ choice: AgentChoice, asOf change: Int) {
        guard change == changes, choice != self.choice else { return }
        self.choice = choice
    }
}
