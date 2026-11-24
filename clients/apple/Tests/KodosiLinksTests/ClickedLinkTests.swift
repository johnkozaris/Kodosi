import Foundation
import KodosiLinks
import Testing

@MainActor
struct ClickedLinkTests {
    @Test func `a link to another app asks first`() throws {
        let editor = try #require(URL(string: "vscode://file/Users/ana/repo/main.rs:12"))
        #expect(ClickedLink("vscode://file/Users/ana/repo/main.rs:12").decision == .ask(editor))
    }

    @Test func `a link with hidden characters does not open and shows them`() {
        let reversed = ClickedLink("https://example.com/\u{202E}gnp.exe")
        #expect(reversed.decision == .refuse(.hiddenCharacters))
        #expect(reversed.shown == "https://example.com/\\u{202E}gnp.exe")
        #expect(ClickedLink("https://example.com/%E2%80%AEtxt").decision == .refuse(.hiddenCharacters))
        #expect(ClickedLink("https://example.com/\nnext").decision == .refuse(.hiddenCharacters))
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
            #expect(ClickedLink(target.absoluteString).decision == .refuse(.program), "\(target)")
        }
    }

    @Test func `a file link that names another computer, a query or no file does not open`() {
        #expect(ClickedLink("file://server/tmp").decision == .refuse(.fileElsewhere("server")))
        #expect(ClickedLink("file:///tmp?run=1").decision == .refuse(.malformed))
        #expect(ClickedLink("file:///nowhere/kodosi-missing.txt").decision == .refuse(.missingFile))
    }

    @Test func `in a terminal of another computer, its files and local addresses do not open here`() throws {
        let pullRequest = try #require(URL(string: "https://github.com/kodosi/kodosi/pull/1"))
        #expect(ClickedLink(pullRequest.absoluteString, otherComputer: "studio").decision == .open(pullRequest))
        let editor = try #require(URL(string: "vscode://file/home/ana/main.rs"))
        #expect(ClickedLink("vscode://file/home/ana/main.rs", otherComputer: "studio").decision == .ask(editor))

        for file in ["file:///home/ana/notes.txt", "/home/ana/notes.txt", "src/main.rs", "~/notes.txt", "file://studio/etc"] {
            #expect(ClickedLink(file, otherComputer: "studio").decision == .refuse(.fileElsewhere("studio")), "\(file)")
        }
        for address in [
            "http://localhost:5173/", "https://127.0.0.1:8443/admin", "http://0.0.0.0:3000",
            "http://[::1]:3000/", "http://app.localhost/",
        ] {
            #expect(ClickedLink(address, otherComputer: "studio").decision == .refuse(.addressElsewhere("studio")), "\(address)")
        }
        #expect(ClickedLink("~/notes.txt", otherComputer: "studio").shown == "~/notes.txt")
        #expect(ClickedLink("file:///home/ana/notes.txt", otherComputer: "studio").shown == "/home/ana/notes.txt")

        let web = try #require(URL(string: "http://127.0.0.1.example.com/"))
        #expect(ClickedLink("http://127.0.0.1.example.com/", otherComputer: "studio").decision == .open(web))
    }

    private func temporaryFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appending(path: "kodosi-link-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}
