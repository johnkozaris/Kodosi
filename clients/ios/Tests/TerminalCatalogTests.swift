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
        let computers = SessionComputerGroup.groups(all)
        #expect(computers.map(\.title) == ["Air", "Studio"])
        #expect(computers.flatMap(\.sessions).map(\.id) == ["notes", "build", "review"])
    }

    @Test func `a friends terminal that waits counts only while you have it open`() throws {
        let all = try sessions([
            terminalEntry("closed", state: "blocked", owner: false, host: "Lab", connection: "offline"),
            terminalEntry("open", state: "blocked", owner: false, host: "Lab"),
        ])
        #expect(TerminalCatalog.needsYou(all).map(\.id) == ["open"])
    }

    @Test func `friends computers follow yours, each in folders, and a terminal that you have not opened has none`() throws {
        var website = terminalEntry("website", state: "idle", host: "Studio")
        website["workingDir"] = "/Users/ana/website"
        var kodosi = terminalEntry("kodosi", state: "working", host: "Studio")
        kodosi["workingDir"] = "/Users/ana/kodosi"
        var opened = terminalEntry("opened", state: "idle", owner: false, host: "Lab")
        opened["workingDir"] = "/home/maria/api"
        var away = terminalEntry("away", state: "idle", owner: false, host: "Box", connection: "offline")
        away["ownerUserId"] = "user-3"
        away["ownerName"] = "Ana"
        away["status"] = "offline"
        let computers = try SessionComputerGroup.groups(sessions([
            terminalEntry("unopened", state: "idle", owner: false, host: "Lab", connection: "offline"), opened, website, kodosi, away,
        ]))
        #expect(computers.map(\.title) == ["Studio", "Ana", "Maria"])
        #expect(computers.map(\.away) == [false, true, false])
        #expect(computers[0].folders.map(\.name) == ["kodosi", "website"])
        #expect(computers[2].folders.map(\.directory) == ["/home/maria/api", nil])
        #expect(computers[2].folders.map { $0.sessions.map(\.id) } == [["opened"], ["unopened"]])
    }

    @Test func `the plus of a folder starts a terminal in it while its computer is online`() throws {
        var mine = terminalEntry("mine", state: "idle", host: "Studio")
        mine["workingDir"] = "/Users/ana/kodosi"
        var theirs = terminalEntry("theirs", state: "idle", owner: false, host: "Lab")
        theirs["workingDir"] = "/home/maria/api"
        let computers = try SessionComputerGroup.groups(sessions([mine, terminalEntry("loose", state: "idle", host: "Studio"), theirs]))
        let studio = try #require(computers.first { $0.title == "Studio" })
        let lab = try #require(computers.first { $0.owner != nil })
        let online = ["device-Studio", "device-Lab"]

        let request = try #require(TerminalCatalog.newTerminal(in: studio.folders[0], online: online))
        #expect(request.computer == "device-Studio")
        #expect(request.folder == "/Users/ana/kodosi")
        #expect(TerminalCatalog.newTerminal(in: studio.folders[0], online: []) == nil)
        #expect(studio.folders[1].directory == nil)
        #expect(TerminalCatalog.newTerminal(in: studio.folders[1], online: online) == nil)
        #expect(TerminalCatalog.newTerminal(in: lab.folders[0], online: online) == nil)
        #expect(TerminalCatalog.newTerminal(on: studio, online: online)?.computer == "device-Studio")
        #expect(TerminalCatalog.newTerminal(on: studio, online: online)?.folder == nil)
        #expect(TerminalCatalog.newTerminal(on: lab, online: online) == nil)
    }

    @Test func `search finds names computers projects and agents`() throws {
        let all = try sessions([
            terminalEntry("deploy", state: "idle", host: "Studio"),
            terminalEntry("tests", state: "idle", host: "Air", project: (id: "project-1", name: "Launch")),
        ])
        #expect(TerminalCatalog.matching(all, query: "DEP").map(\.id) == ["deploy"])
        #expect(TerminalCatalog.matching(all, query: "air").map(\.id) == ["tests"])
        #expect(TerminalCatalog.matching(all, query: "launch").map(\.id) == ["tests"])
        #expect(TerminalCatalog.matching(all, query: "claude").count == 2)
        #expect(TerminalCatalog.matching(all, query: "  ").count == 2)
        #expect(TerminalCatalog.matching(all, query: "nothing").isEmpty)
    }
}
