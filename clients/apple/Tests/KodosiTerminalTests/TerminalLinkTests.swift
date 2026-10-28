import Foundation
@testable import KodosiTerminal
import Testing

@MainActor
struct TerminalLinkTests {
    @Test func `web and mail links open at once`() throws {
        let web = try #require(URL(string: "https://example.com/path?q=1"))
        let mail = try #require(URL(string: "mailto:ana@example.com"))
        #expect(TerminalLink("https://example.com/path?q=1").decision == .open(web))
        #expect(TerminalLink("mailto:ana@example.com").decision == .open(mail))
    }

    @Test func `a web link without a host or a mail link without an address does not open`() {
        #expect(TerminalLink("https:relative").decision == .refuse(.noWebHost))
        #expect(TerminalLink("mailto:").decision == .refuse(.malformed))
        #expect(TerminalLink("").decision == .refuse(.malformed))
    }

    @Test func `a link to another app asks first`() throws {
        let editor = try #require(URL(string: "vscode://file/Users/ana/repo/main.rs:12"))
        #expect(TerminalLink("vscode://file/Users/ana/repo/main.rs:12").decision == .ask(editor))
    }

    @Test func `a link with hidden characters does not open and shows them`() {
        let reversed = TerminalLink("https://example.com/\u{202E}gnp.exe")
        #expect(reversed.decision == .refuse(.hiddenCharacters))
        #expect(reversed.shown == "https://example.com/\\u{202E}gnp.exe")
        #expect(TerminalLink("https://example.com/%E2%80%AEtxt").decision == .refuse(.hiddenCharacters))
        #expect(TerminalLink("https://example.com/\nnext").decision == .refuse(.hiddenCharacters))
    }

    @Test func `a folder and a document open by their real path`() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let notes = folder.appending(path: "notes.txt")
        try Data("notes".utf8).write(to: notes)
        let real = path(folder.resolvingSymlinksInPath())

        #expect(openedPath(TerminalLink(folder.absoluteString)) == real)
        #expect(openedPath(TerminalLink(notes.absoluteString)) == "\(real)/notes.txt")
        #expect(openedPath(TerminalLink(path(notes))) == "\(real)/notes.txt")
        #expect(TerminalLink(notes.absoluteString).shown == "\(real)/notes.txt")
    }

    @Test func `a file that can run code does not open`() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let script = folder.appending(path: "build")
        try Data("#!/bin/sh\necho hi\n".utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path(percentEncoded: false))
        let command = folder.appending(path: "setup.command")
        try Data("echo hi\n".utf8).write(to: command)
        let app = folder.appending(path: "Tool.app", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: app, withIntermediateDirectories: true)
        let disguise = folder.appending(path: "readme.txt")
        try FileManager.default.createSymbolicLink(at: disguise, withDestinationURL: script)

        for target in [script, command, app, disguise] {
            #expect(TerminalLink(target.absoluteString).decision == .refuse(.program), "\(target)")
        }
    }

    @Test func `a file link that names another computer, a query or no file does not open`() {
        #expect(TerminalLink("file://server/tmp").decision == .refuse(.fileElsewhere("server")))
        #expect(TerminalLink("file:///tmp?run=1").decision == .refuse(.malformed))
        #expect(TerminalLink("file:///nowhere/kodosi-missing.txt").decision == .refuse(.missingFile))
    }

    @Test func `a file link that names this computer opens`() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let notes = folder.appending(path: "notes.txt")
        try Data("notes".utf8).write(to: notes)
        let real = path(folder.resolvingSymlinksInPath())

        #expect(openedPath(TerminalLink("file://\(hostName())\(path(notes))")) == "\(real)/notes.txt")
        #expect(openedPath(TerminalLink("file://LOCALHOST\(path(notes))")) == "\(real)/notes.txt")
    }

    @Test func `in a terminal of another computer, its files and local addresses do not open here`() throws {
        let pullRequest = try #require(URL(string: "https://github.com/kodosi/kodosi/pull/1"))
        #expect(TerminalLink(pullRequest.absoluteString, otherComputer: "studio").decision == .open(pullRequest))
        let editor = try #require(URL(string: "vscode://file/home/ana/main.rs"))
        #expect(TerminalLink("vscode://file/home/ana/main.rs", otherComputer: "studio").decision == .ask(editor))

        for file in ["file:///home/ana/notes.txt", "/home/ana/notes.txt", "src/main.rs", "~/notes.txt", "file://studio/etc"] {
            #expect(TerminalLink(file, otherComputer: "studio").decision == .refuse(.fileElsewhere("studio")), "\(file)")
        }
        for address in [
            "http://localhost:5173/", "https://127.0.0.1:8443/admin", "http://0.0.0.0:3000",
            "http://[::1]:3000/", "http://app.localhost/",
        ] {
            #expect(TerminalLink(address, otherComputer: "studio").decision == .refuse(.addressElsewhere("studio")), "\(address)")
        }
        #expect(TerminalLink("~/notes.txt", otherComputer: "studio").shown == "~/notes.txt")
        #expect(TerminalLink("file:///home/ana/notes.txt", otherComputer: "studio").shown == "/home/ana/notes.txt")
        #expect(TerminalLink.Refusal.fileElsewhere("studio").message == "The file is on studio.")

        let local = try #require(URL(string: "http://localhost:5173/"))
        #expect(TerminalLink("http://localhost:5173/").decision == .open(local))
        #expect(TerminalLink("http://127.0.0.1.example.com/", otherComputer: "studio").decision != .refuse(.addressElsewhere("studio")))
    }

    @Test func `the proxy opens a safe link and lets the view review every other link`() throws {
        let proxy = TerminalRendererDelegateProxy()
        var opened: [URL] = []
        var reviewed: [TerminalLink] = []
        proxy.opensLink = { opened.append($0) }
        proxy.linkReviewRequested = { reviewed.append($0) }

        proxy.terminalDidRequestOpenURL("https://example.com", kind: .unknown)
        proxy.terminalDidRequestOpenURL("zoommtg://join?confno=1", kind: .unknown)
        proxy.terminalDidRequestOpenURL("file:///nowhere/kodosi-missing.txt", kind: .unknown)

        #expect(try opened == [#require(URL(string: "https://example.com"))])
        #expect(try reviewed.map(\.decision) == [
            .ask(#require(URL(string: "zoommtg://join?confno=1"))),
            .refuse(.missingFile),
        ])

        proxy.otherComputer = "studio"
        proxy.terminalDidRequestOpenURL("http://localhost:5173/", kind: .unknown)
        #expect(reviewed.last?.decision == .refuse(.addressElsewhere("studio")))
        proxy.terminalDidRequestOpenURL("/private/tmp/screen.txt", kind: .text)
        #expect(opened.last == URL(filePath: "/private/tmp/screen.txt"))
        #expect(reviewed.count == 3)
    }

    private func openedPath(_ link: TerminalLink) -> String? {
        guard case let .open(url) = link.decision else { return nil }
        return path(url)
    }

    private func path(_ url: URL) -> String {
        let path = url.path(percentEncoded: false)
        return path.count > 1 && path.hasSuffix("/") ? String(path.dropLast()) : path
    }

    private func hostName() -> String {
        var name = [CChar](repeating: 0, count: 256)
        gethostname(&name, name.count)
        return String(utf8String: name) ?? ""
    }

    private func temporaryFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appending(path: "kodosi-link-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}
