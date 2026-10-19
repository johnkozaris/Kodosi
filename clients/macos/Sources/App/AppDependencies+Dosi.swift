import Foundation
import KodosiBridge
import KodosiDesign

extension AppDependencies {
    func touch(by item: DosiItem) {
        guard case let .tool(tool) = item.body, let target = tool.target, target.kind == .terminal,
              let session = terminal(named: target.value) else { return }
        if dosiTouched.insert(item.id).inserted {
            if dosiTouched.count > 512 {
                dosiTouched = [item.id]
            }
            dosiTouches[session.id] = DosiTouch(count: (dosiTouches[session.id]?.count ?? 0) + 1, item: item.id, tool: tool)
        } else if dosiTouches[session.id]?.item == item.id {
            dosiTouches[session.id]?.tool = tool
        }
    }

    var dosiContext: DosiContext {
        switch workbench.section {
        case .sessions:
            return DosiContext(terminal: workbench.focusedSessionId ?? workbench.selectedSessionId)
        case .missions:
            guard let room = workbench.selectedMissionId else { return DosiContext() }
            return DosiContext(terminal: roomView(room).selectedTerminal, room: room)
        case .people, .settings:
            return DosiContext()
        }
    }

    var dosiAgentLabel: String? {
        dosi.snapshot?.setup.agent.map { AgentKind(program: $0).label }
    }

    var dosiStatus: String {
        switch dosi.state {
        case .unavailable: return String(localized: "Choose its agent in Settings")
        case .connecting: return String(localized: "Connecting")
        case .stopping: return String(localized: "Stopping")
        case .idle: return dosiAgentLabel ?? String(localized: "Ready")
        case .working:
            if let remote = dosi.snapshot?.remote {
                return String(localized: "Working on \(remote.computer)")
            }
            return dosi.mood().label
        }
    }

    var dosiDeed: String? {
        if dosi.state == .working, let remote = dosi.snapshot?.remote {
            return String(localized: "on \(remote.computer)")
        }
        return dosi.latestDeed.map(DosiWords.doing)
    }

    func sendToDosi(_ text: String? = nil) {
        let message = (text ?? dosi.draft).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty else { return }
        let typed = text == nil
        if typed {
            dosi.draft = ""
        }
        let context = dosiContext
        Task { @MainActor in
            do {
                try await dosi.send(message, context: context)
            } catch {
                if typed, dosi.draft.isEmpty {
                    dosi.draft = message
                }
                errorMessage = error.localizedDescription
            }
        }
    }

    func answerDosi(_ item: DosiItem, _ choices: [String]) {
        Task { @MainActor in
            do {
                try await dosi.answer(item, choices: choices)
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func renewDosi() {
        Task { @MainActor in
            do {
                try await dosi.renew()
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func terminal(named reference: String) -> RuntimeSession? {
        sessions.first { $0.id == reference }
            ?? sessions.first { $0.name.caseInsensitiveCompare(reference) == .orderedSame }
            ?? (reference.count >= 4 ? sessions.first { $0.id.hasPrefix(reference) } : nil)
    }

    func room(named reference: String) -> MissionEntry? {
        missions.first { $0.id == reference } ?? missions.first { $0.name.caseInsensitiveCompare(reference) == .orderedSame }
    }

    func open(_ target: DosiTarget) {
        switch target.kind {
        case .terminal:
            if let session = terminal(named: target.value) {
                activateSession(session.id)
            }
        case .room:
            if let mission = room(named: target.value) {
                workbench.section = .missions
                openMission(mission)
            }
        case .rooms:
            showRooms()
        case .terminals:
            workbench.section = .sessions
        case .url:
            if let url = URL(string: target.value), ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
                openExternalURL(url)
            }
        case .agent, .file, .query, .command, .tool:
            break
        }
    }
}
