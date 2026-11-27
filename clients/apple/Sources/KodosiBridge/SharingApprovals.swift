import Foundation
import Observation

/// A change that lets someone new into the owner's terminals or projects, waiting for the owner's
/// answer. The runtime words the question; the apps ask it as it is.
public struct SharingApproval: Decodable, Equatable, Identifiable, Sendable {
    public let approvalId: String
    public let operation: String
    public let title: String
    public let message: String
    public let consent: String
    public let confirm: String
    /// Who asked, when the question did not come from a person in the app.
    public let asker: String?
    /// How long the question shows while Kodosi is in front before its confirm button works.
    public let readSeconds: Int

    public var id: String {
        approvalId
    }
}

/// The open questions in the order they came. The apps ask one at a time, oldest first.
@MainActor
@Observable
public final class SharingApprovals {
    /// How long a command can take to answer when it waits for the owner: five minutes for the
    /// owner's answer, and then the time that the command itself takes.
    public static let longestWait = Duration.seconds(5 * 60 + 30)

    public private(set) var waiting: [SharingApproval] = []
    @ObservationIgnored public var notify: (String) -> Void = { _ in }
    @ObservationIgnored private let commands: CommandSink
    /// The runtime sends its open questions again when an app joins or resyncs, so a question
    /// whose answer is on its way must not be asked again.
    @ObservationIgnored private var answering: Set<String> = []

    public init(commands: CommandSink) {
        self.commands = commands
    }

    public var current: SharingApproval? {
        waiting.first
    }

    public func reset() {
        waiting = []
    }

    public func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "sharing.approval":
            let approval = try JSONValue.object(event.payload).decode(as: SharingApproval.self)
            guard !answering.contains(approval.id), !waiting.contains(where: { $0.id == approval.id }) else { return }
            waiting.append(approval)
        case "sharing.approvalEnded":
            let id: String = try event.value("approvalId")
            waiting.removeAll { $0.id == id }
        default:
            break
        }
    }

    public func answer(_ approval: SharingApproval, allow: Bool) {
        guard let index = waiting.firstIndex(where: { $0.id == approval.id }) else { return }
        waiting.remove(at: index)
        answering.insert(approval.id)
        Task {
            defer { answering.remove(approval.id) }
            do {
                _ = try await commands.request("sharing.answer", ["approvalId": .string(approval.id), "allow": .bool(allow)])
            } catch {
                notify(error.localizedDescription)
            }
        }
    }
}
