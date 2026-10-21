import Foundation
@testable import Kodosi
import KodosiBridge
import Testing

struct TerminalCatalogTests {
    private func sessions(_ entries: [[String: Any]]) throws -> [RuntimeSession] {
        try JSONDecoder().decode([RuntimeSession].self, from: JSONSerialization.data(withJSONObject: entries))
    }

    @Test func `terminals that wait stay with their computer`() throws {
        let all = try sessions([
            terminalEntry("build", state: "working", host: "Studio"),
            terminalEntry("review", state: "blocked", host: "Studio"),
            terminalEntry("notes", state: "idle", host: "Air"),
        ])
        #expect(TerminalCatalog.needsYou(all).map(\.id) == ["review"])
        let groups = TerminalCatalog.groups(all)
        #expect(groups.map(\.title) == ["Air", "Studio"])
        #expect(groups.flatMap(\.sessions).map(\.id) == ["notes", "build", "review"])
    }

    @Test func `a friends terminal that waits counts only while you have it open`() throws {
        let all = try sessions([
            terminalEntry("closed", state: "blocked", owner: false, host: "Lab", connection: "offline"),
            terminalEntry("open", state: "blocked", owner: false, host: "Lab"),
        ])
        #expect(TerminalCatalog.needsYou(all).map(\.id) == ["open"])
        #expect(TerminalCatalog.groups(all).flatMap(\.sessions).map(\.id) == ["closed", "open"])
    }

    @Test func `friends terminals are shared with you after your computers`() throws {
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

    @Test func `search finds names computers rooms and agents`() throws {
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
