import Foundation
import KodosiBridge
import KodosiDesign

enum LauncherMenu: String, Equatable {
    case computer, folder, account, model

    enum Pick: Equatable {
        case computer(String?), folder(String), anotherFolder, account(String), newAccount, model(String?)
    }

    static let add = "menu.add"
    static let choose = "menu.choose"
    static let standard = "menu.default"
    static let here = "menu.here"

    func pick(_ id: String) -> Pick {
        switch self {
        case .computer: .computer(id == Self.here ? nil : id)
        case .folder: id == Self.choose ? .anotherFolder : .folder(id)
        case .account: id == Self.add ? .newAccount : .account(id)
        case .model: .model(id == Self.standard ? nil : id)
        }
    }

    @MainActor
    func rows(_ plan: LaunchPlan, recents: [String], directory: String?, loading: Bool) -> [PickerRow] {
        switch self {
        case .computer:
            []
        case .folder:
            Self.folders(recents, current: directory)
        case .account:
            plan.info.map { Self.accounts($0, current: plan.account?.name ?? "") { plan.usage(of: $0) } } ?? []
        case .model:
            Self.models(plan.models?.models, agent: plan.info?.kind.label ?? "", loading: loading, current: plan.modelId)
        }
    }

    static func computers(_ computers: [MyDeviceEntry], current: String?) -> [PickerRow] {
        let here = String(localized: "This Mac")
        return [PickerRow(id: Self.here, title: here, icon: .device(here), current: current == nil)]
            + computers.map { PickerRow(id: $0.deviceId, title: $0.label, icon: .device($0.label), current: $0.deviceId == current) }
    }

    static func accounts(
        _ info: AgentInfo, current: String, usage: (AgentAccount) -> UsageLimit? = { _ in nil },
    ) -> [PickerRow] {
        var rows = info.accounts.map { account in
            let title = account.title(program: info.program)
            return PickerRow(
                id: account.name, title: title,
                subtitle: account.identity ?? String(localized: "Signs in when it starts"),
                icon: .account(initial: String(title.prefix(1)), key: account.signIn, signedIn: account.identity != nil),
                usage: usage(account),
                current: account.name == current,
            )
        }
        if info.addsAccounts {
            rows.append(PickerRow(
                id: add, kind: .action, title: String(localized: "Add an account"),
                subtitle: String(localized: "Another subscription with its own sign-in"), icon: .slot("plus"),
            ))
        }
        return rows
    }

    static func models(_ models: [AgentModel]?, agent: String, loading: Bool, current: String?) -> [PickerRow] {
        var rows = [PickerRow(id: standard, title: String(localized: "Default"), current: current == nil)]
        if models == nil {
            rows.append(PickerRow(
                id: "menu.models", kind: .note,
                title: loading ? String(localized: "Asking \(agent) for its models") : String(localized: "\(agent) lists no models"),
            ))
        }
        rows += (models ?? []).map { model in
            PickerRow(
                id: model.id, title: model.label,
                detail: model.label.caseInsensitiveCompare(model.id) == .orderedSame ? nil : model.id,
                current: model.id == current,
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
func recentFolders(_ deps: AppDependencies, chosen: String?, saved: [String]) -> [String] {
    let home = deps.storageBootstrap.defaultWorkingDirectory?.path
    var seen = Set([home].compactMap(\.self))
    let local: [String?] = deps.sessions.filter { $0.kind == .local }.map(\.localDirectory)
    let candidates: [String?] = [chosen, deps.settings.lastWorkingDir] + saved + local
    return Array(candidates.compactMap(\.self).filter { seen.insert($0).inserted }.prefix(7)) + [home].compactMap(\.self)
}

func folderName(_ path: String) -> String {
    tilde(path) == "~" ? "~" : HostPath.folderName(path)
}

func tilde(_ path: String) -> String {
    let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
    return path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}
