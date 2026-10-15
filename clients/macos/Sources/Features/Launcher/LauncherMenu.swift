import Foundation
import KodosiBridge

enum LauncherMenu: String, Equatable {
    case folder, account, model, effort

    enum Pick: Equatable {
        case folder(String), anotherFolder, account(String), newAccount, model(String?), effort(String?)
    }

    static let add = "menu.add"
    static let choose = "menu.choose"
    static let standard = "menu.default"

    static let promptHints = [
        String(localized: "fix the failing test"),
        String(localized: "explain how this folder works"),
        String(localized: "review my last commit"),
        String(localized: "add a dark mode"),
        String(localized: "write tests for the parser"),
    ]
    static let commandHints = ["git status", "npm run dev", "make test", "ls -la"]

    func pick(_ id: String) -> Pick {
        switch self {
        case .folder: id == Self.choose ? .anotherFolder : .folder(id)
        case .account: id == Self.add ? .newAccount : .account(id)
        case .model: .model(id == Self.standard ? nil : id)
        case .effort: .effort(id == Self.standard ? nil : id)
        }
    }

    @MainActor
    func lines(_ plan: LaunchPlan, recents: [String], directory: String?, loading: Bool) -> [TerminalMenu.Line] {
        switch self {
        case .folder:
            Self.folders(recents, current: directory)
        case .account:
            plan.info.map { Self.accounts($0, current: plan.account?.command ?? "") } ?? []
        case .model:
            Self.models(plan.models?.models, loading: loading, current: plan.modelArgument)
        case .effort:
            Self.efforts(plan.efforts, current: plan.effortArgument)
        }
    }

    static func accounts(_ info: AgentInfo, current: String) -> [TerminalMenu.Line] {
        var groups: [(folder: String?, accounts: [AgentAccount])] = []
        for account in info.accounts {
            if let index = groups.firstIndex(where: { $0.folder == account.folder }) {
                groups[index].accounts.append(account)
            } else {
                groups.append((account.folder, [account]))
            }
        }
        let folders = info.accounts.contains { $0.folder != nil }
        var lines: [TerminalMenu.Line] = []
        for group in groups {
            if folders {
                let identity = group.accounts.lazy.compactMap(\.identity).first
                lines.append(TerminalMenu.Line(
                    id: "sign-in.\(group.folder ?? "")", kind: .comment,
                    title: group.folder == nil ? String(localized: "a sign-in that your shell chooses")
                        : identity ?? String(localized: "not signed in yet"),
                    detail: group.folder
                ))
            }
            lines += group.accounts.map { account in
                TerminalMenu.Line(id: account.command, title: account.name, detail: account.options, current: account.command == current)
            }
        }
        if info.addsAccounts {
            lines.append(TerminalMenu.Line(id: add, kind: .action, title: String(localized: "+ new account")))
        }
        return lines
    }

    static func models(_ models: [AgentModel]?, loading: Bool, current: String?) -> [TerminalMenu.Line] {
        var lines = [TerminalMenu.Line(
            id: standard, title: String(localized: "default"), detail: String(localized: "the agent's own choice"), current: current == nil
        )]
        if models == nil {
            lines.append(TerminalMenu.Line(
                id: "menu.models", kind: .comment,
                title: loading ? String(localized: "asking the agent for its models") : String(localized: "the agent did not list its models")
            ))
        }
        lines += (models ?? []).map { model in
            TerminalMenu.Line(
                id: model.argument, title: model.label,
                detail: model.label.caseInsensitiveCompare(model.id) == .orderedSame ? nil : model.id,
                current: model.argument == current
            )
        }
        return lines
    }

    static func efforts(_ efforts: [AgentOption], current: String?) -> [TerminalMenu.Line] {
        [TerminalMenu.Line(
            id: standard, title: String(localized: "default"), detail: String(localized: "the model's own level"), current: current == nil
        )] + efforts.map { TerminalMenu.Line(id: $0.argument, title: $0.label.lowercased(), current: $0.argument == current) }
    }

    static func folders(_ recents: [String], current: String?) -> [TerminalMenu.Line] {
        recents.map { TerminalMenu.Line(id: $0, title: tilde($0), current: $0 == current) }
            + [TerminalMenu.Line(id: choose, kind: .action, title: String(localized: "… another folder"))]
    }
}

@MainActor
func recentFolders(_ deps: AppDependencies, chosen: String?) -> [String] {
    var seen = Set<String>()
    let local: [String?] = deps.sessions.filter { $0.kind == .local }.map(\.localDirectory)
    let candidates: [String?] = [chosen, deps.settings.lastWorkingDir] + local + [deps.storageBootstrap.defaultWorkingDirectory?.path]
    return Array(candidates.compactMap(\.self).filter { seen.insert($0).inserted }.prefix(6))
}

func folderName(_ path: String) -> String {
    tilde(path) == "~" ? "~" : URL(fileURLWithPath: path).lastPathComponent
}

func tilde(_ path: String) -> String {
    let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
    return path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}
