import Foundation
import Observation

/// When a computer holds off the sleep that starts by itself.
public enum KeepAwake: String, CaseIterable, Decodable, Sendable {
    /// While an agent works on it, and for a few minutes after.
    case agents
    case always
    case never
}

/// How one computer of the person wakes, as the runtime knows it now.
public struct ComputerPower: Decodable, Equatable, Sendable {
    public enum Busy: String, Decodable, Sendable {
        case waking
        case testing
    }

    public let deviceId: String
    /// What the person can expect of a wake, or the one setting to change for it.
    public let note: String?
    /// Why the computer does not wake now, with the one thing to do.
    public let problem: String?
    public let canWake: Bool
    public let canTest: Bool
    public let busy: Busy?
    public let keepAwake: KeepAwake
}

@MainActor
@Observable
public final class PowerStore {
    /// `nil` on a device that hosts no terminals.
    public private(set) var keepsAwake: KeepAwake?
    public private(set) var computers: [String: ComputerPower] = [:]
    @ObservationIgnored private let commands: CommandSink

    public init(commands: CommandSink) {
        self.commands = commands
    }

    public func receive(_ event: RuntimeEvent) throws {
        if event.type == "power.state" {
            keepsAwake = try event.payload["keepAwake"].map { try $0.decode(as: KeepAwake.self) }
            let listed = try event.value("computers", as: [ComputerPower].self)
            computers = Dictionary(listed.map { ($0.deviceId, $0) }) { _, later in later }
        }
    }

    /// The computers that sleep and that a wake can reach now, and the ones that wake now.
    public func asleep(among devices: [MyDeviceEntry]) -> [MyDeviceEntry] {
        devices.filter { device in computers[device.deviceId].map { $0.canWake || $0.busy == .waking } ?? false }
            .sorted { $0.label.localizedStandardCompare($1.label) == .orderedAscending }
    }

    public func keepAwake(_ choice: KeepAwake) {
        keepsAwake = choice
        commands.perform("power.keepAwake", ["keepAwake": .string(choice.rawValue)])
    }

    public func keepAwake(_ choice: KeepAwake, on computer: String) {
        commands.perform("power.keepAwake", ["keepAwake": .string(choice.rawValue), "computer": .string(computer)])
    }

    public func sleep(_ computer: MyDeviceEntry) {
        commands.perform("power.sleep", ["computer": .string(computer.deviceId)])
    }

    public func wake(_ deviceId: String) {
        commands.perform("power.wake", ["deviceId": .string(deviceId)])
    }

    public func testWake(_ computer: MyDeviceEntry) {
        commands.perform("power.test", ["deviceId": .string(computer.deviceId)])
    }
}
