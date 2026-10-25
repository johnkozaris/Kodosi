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

    var dosiLine: String {
        switch dosi.state {
        case .unavailable: return String(localized: "Choose Dosi's agent in Settings")
        case .stopping: return String(localized: "Dosi is stopping")
        case .working:
            if let remote = dosi.snapshot?.remote {
                return String(localized: "Dosi is working on \(remote.computer)")
            }
            return String(localized: "Dosi is working")
        case .idle, .connecting:
            if !dosi.openQuestions.isEmpty {
                return String(localized: "Dosi has a question")
            }
            return dosi.unseen ? String(localized: "Dosi answered") : String(localized: "Ask Dosi")
        }
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
        sessions.first { refers(reference, to: $0.id, named: $0.name) }
    }

    func room(named reference: String) -> MissionEntry? {
        missions.first { refers(reference, to: $0.id, named: $0.name) }
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
