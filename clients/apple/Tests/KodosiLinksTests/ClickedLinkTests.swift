import Foundation
@testable import KodosiLinks
import Testing

struct ClickedLinkTests {
    @Test func `a file that can run code does not open, and a document does`() throws {
        let folder = FileManager.default.temporaryDirectory
            .appending(path: "kodosi-link-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
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
        let notes = folder.appending(path: "notes.txt")
        try Data("notes".utf8).write(to: notes)

        for target in [script, command, app, disguise] {
            #expect(ClickedLink.runsCode(target), "\(target)")
        }
        #expect(!ClickedLink.runsCode(notes))
        #expect(!ClickedLink.runsCode(folder))
    }
}
