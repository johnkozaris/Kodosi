import Foundation
import Observation

/// When a computer holds off the sleep that starts by itself.
public enum KeepAwake: String, Decodable, Sendable {
    /// While an agent works on it, and for a few minutes after.
    case agents
    case always
    case never
}

@MainActor
@Observable
public final class PowerStore {
    /// `nil` on a device that hosts no terminals.
    public private(set) var keepsAwake: KeepAwake?

    @ObservationIgnored private let commands: CommandSink

    public init(commands: CommandSink) {
        self.commands = commands
    }

    public func receive(_ event: RuntimeEvent) throws {
        if event.type == "power.state" {
            keepsAwake = try event.payload["keepAwake"].map { try $0.decode(as: KeepAwake.self) }
        }
    }

    public func keepAwake(_ choice: KeepAwake) {
        keepsAwake = choice
        commands.perform("power.keepAwake", ["keepAwake": .string(choice.rawValue)])
    }

    public func sleep(_ computer: MyDeviceEntry) {
        commands.perform("power.sleep", ["computer": .string(computer.deviceId)])
    }
}
