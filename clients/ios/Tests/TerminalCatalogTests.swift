import Foundation
@testable import Kodosi
import KodosiBridge
import Testing

struct TerminalCatalogTests {
    private func sessions(_ entries: [[String: Any]]) throws -> [RuntimeSession] {
        try JSONDecoder().decode([RuntimeSession].self, from: JSONSerialization.data(withJSONObject: entries))
    }

    @Test func terminalsThatWaitComeFirstAndLeaveTheirComputer() throws {
        let all = try sessions([
            terminalEntry("build", state: "working", host: "Studio"),
            terminalEntry("review", state: "blocked", host: "Studio"),
            terminalEntry("notes", state: "idle", host: "Air"),
        ])
        #expect(TerminalCatalog.needsYou(all).map(\.id) == ["review"])
        let groups = TerminalCatalog.groups(all)
        #expect(groups.map(\.title) == ["Air", "Studio"])
        #expect(groups.flatMap(\.sessions).map(\.id) == ["notes", "build"])
    }

    @Test func friendsTerminalsAreSharedWithYouAfterYourComputers() throws {
        let all = try sessions([
            terminalEntry("theirs", state: "idle", owner: false, host: "Maria's Mac"),
            terminalEntry("mine", state: "idle", host: "Studio"),
            terminalEntry("room", state: "working", owner: false, host: "Lab", room: (id: "room-1", name: "Launch")),
        ])
        let groups = TerminalCatalog.groups(all)
        #expect(groups.map(\.place) == [.computer(id: "device-Studio", name: "Studio"), .shared])
        #expect(groups.last?.sessions.map(\.id) == ["theirs", "room"])
        #expect(groups.last?.sessions.last?.missionName == "Launch")
    }

    @Test func searchFindsNamesComputersRoomsAndAgents() throws {
        let all = try sessions([
            terminalEntry("deploy", state: "idle", host: "Studio"),
            terminalEntry("tests", state: "idle", host: "Air", room: (id: "room-1", name: "Launch")),
        ])
        #expect(TerminalCatalog.matching(all, query: "DEP").map(\.id) == ["deploy"])
        #expect(TerminalCatalog.matching(all, query: "air").map(\.id) == ["tests"])
        #expect(TerminalCatalog.matching(all, query: "launch").map(\.id) == ["tests"])
        #expect(TerminalCatalog.matching(all, query: "claude").count == 2)
        #expect(TerminalCatalog.matching(all, query: "  ").count == 2)
        #expect(TerminalCatalog.matching(all, query: "nothing").isEmpty)
    }
}
