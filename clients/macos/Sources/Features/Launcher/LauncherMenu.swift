import Foundation
import KodosiBridge
import KodosiDesign

enum LauncherMenu: String, Equatable {
    case folder, account, model

    enum Pick: Equatable {
        case folder(String), anotherFolder, account(String), newAccount, model(String?)
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
        }
    }

    @MainActor
    func rows(_ plan: LaunchPlan, recents: [String], directory: String?, loading: Bool) -> [PickerRow] {
        switch self {
        case .folder:
            Self.folders(recents, current: directory)
        case .account:
            plan.info.map { Self.accounts($0, current: plan.account?.command ?? "") } ?? []
        case .model:
            Self.models(plan.models?.models, agent: plan.info?.kind.label ?? "", loading: loading, current: plan.modelArgument)
        }
    }

    static func accounts(_ info: AgentInfo, current: String) -> [PickerRow] {
        var rows = info.accounts.map { account in
            let title = account.title(program: info.program)
            return PickerRow(
                id: account.command, title: title,
                subtitle: account.identity ?? (account.folder == nil
                    ? String(localized: "Your shell chooses the sign-in") : String(localized: "Signs in when it starts")),
                detail: account.options,
                icon: .account(initial: String(title.prefix(1)), key: account.signIn, signedIn: account.identity != nil),
                current: account.command == current
            )
        }
        if info.addsAccounts {
            rows.append(PickerRow(
                id: add, kind: .action, title: String(localized: "Add an account"),
                subtitle: String(localized: "Another subscription with its own sign-in"), icon: .slot("plus")
            ))
        }
        return rows
    }

    static func models(_ models: [AgentModel]?, agent: String, loading: Bool, current: String?) -> [PickerRow] {
        var rows = [PickerRow(id: standard, title: String(localized: "Default"), current: current == nil)]
        if models == nil {
            rows.append(PickerRow(
                id: "menu.models", kind: .note,
                title: loading ? String(localized: "Asking \(agent) for its models") : String(localized: "\(agent) lists no models")
            ))
        }
        rows += (models ?? []).map { model in
            PickerRow(
                id: model.argument, title: model.label,
                detail: model.label.caseInsensitiveCompare(model.id) == .orderedSame ? nil : model.id,
                current: model.argument == current
            )
        }
        return rows
    }

    static func folders(_ recents: [String], current: String?) -> [PickerRow] {
        recents.map { PickerRow(id: $0, title: folderName($0), subtitle: tilde($0), icon: .folder, current: $0 == current) }
            + [PickerRow(id: choose, kind: .action, title: String(localized: "Choose a folder…"), icon: .slot("folder"))]
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
    tilde(path) == "~" ? "~" : HostPath.folderName(path)
}

func tilde(_ path: String) -> String {
    let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
    return path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}
