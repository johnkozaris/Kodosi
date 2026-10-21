import Foundation
@testable import Kodosi
import KodosiBridge
import Testing

struct SettingsTests {
    @Test(arguments: [
        ("abcd efgh jkmn", "ABCD-EFGH-JKMN"),
        ("ABCD-EFGH-JKMN-PQRS", "ABCD-EFGH-JKMN"),
        ("ab-c", "ABC"),
        ("abcd-", "ABCD"),
        ("abcde", "ABCD-E"),
        ("", ""),
    ])
    func `device code is grouped as the other device shows it`(typed: String, shown: String) {
        #expect(DeviceCode.format(typed) == shown)
    }

    @Test func `device code is complete with twelve symbols`() {
        #expect(DeviceCode.isComplete("abcd efgh jkmn"))
        #expect(!DeviceCode.isComplete("ABCD-EFGH-JKM"))
        #expect(!DeviceCode.isComplete("ABCD-EFGH-JKM!"))
    }

    @Test func `license titles come from the file name`() throws {
        let url = try #require(URL(string: "file:///Notices/LobeHub-Icons-MIT.txt"))
        #expect(LicenseShelf.title(for: url) == "LobeHub Icons MIT")
        let notice = try #require(URL(string: "file:///Notices/THIRD_PARTY_NOTICES.md"))
        #expect(LicenseShelf.title(for: notice) == "THIRD PARTY NOTICES")
    }

    @Test func `license prose reflows and lists keep their lines`() {
        let text = """
        MIT License

        Permission is hereby granted, free of charge,
        to any person obtaining a copy.

        1. Keep this notice.
        2. Keep that notice.
          indented line
        """
        #expect(LicenseShelf.paragraphs(in: text) == [
            "MIT License",
            "Permission is hereby granted, free of charge, to any person obtaining a copy.",
            "1. Keep this notice.\n2. Keep that notice.\n  indented line",
        ])
    }

    @Test func `markdown notices keep headings and inline code`() {
        #expect(LicenseShelf.block("## Bundled software", markdown: true) == .heading("Bundled software"))
        guard case let .text(text) = LicenseShelf.block("Uses `zlib` here.", markdown: true) else {
            Issue.record("A paragraph is text")
            return
        }
        #expect(String(text.characters) == "Uses zlib here.")
        #expect(LicenseShelf.block("# Not a heading in plain text", markdown: false) == .text(AttributedString("# Not a heading in plain text")))
    }

    @Test func `missing notices folder lists nothing`() async {
        #expect(await LicenseShelf.files(in: nil).isEmpty)
        #expect(await LicenseShelf.files(in: URL(fileURLWithPath: "/nonexistent/Notices")).isEmpty)
    }
}
