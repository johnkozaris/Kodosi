import Foundation
import KodosiBridge

extension AppModel {
    /// The computer that answered last stays in the list while it is away, because a message
    /// waits for it.
    var dosiComputers: [DosiComputer] {
        var computers = account.computers.map { DosiComputer(device: $0.deviceId, name: $0.label) }
        if let last = dosi.snapshot?.last, !computers.contains(where: { $0.device == last.device }),
           let device = account.devices.first(where: { $0.deviceId == last.device && !$0.recoveryKey })
        {
            computers.append(DosiComputer(device: device.deviceId, name: device.label))
        }
        return computers
    }

    var dosiComputer: DosiComputer? {
        let computers = dosiComputers
        return computers.first { $0.device == dosiDevice }
            ?? computers.first { $0.device == dosi.snapshot?.last?.device }
            ?? computers.first
    }

    func dosiAway(_ computer: DosiComputer) -> Bool {
        !account.computers.contains { $0.deviceId == computer.device }
    }

    var dosiConnected: Bool {
        guard let snapshot = dosi.snapshot else { return false }
        return snapshot.shared && snapshot.state != .connecting
    }

    func sendToDosi(_ text: String? = nil) {
        let message = (text ?? dosi.draft).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty, let computer = dosiComputer else { return }
        let typed = text == nil
        if typed {
            dosi.draft = ""
        }
        Task {
            do {
                try await dosi.send(message, context: DosiContext(), to: computer)
            } catch {
                if typed, dosi.draft.isEmpty {
                    dosi.draft = message
                }
                notices.show(error.localizedDescription)
            }
        }
    }

    func answerDosi(_ item: DosiItem, _ choices: [String]) {
        Task {
            do {
                try await dosi.answer(item, choices: choices)
            } catch { notices.show(error.localizedDescription) }
        }
    }

    func renewDosi() {
        Task {
            do {
                try await dosi.renew()
            } catch { notices.show(error.localizedDescription) }
        }
    }
}
