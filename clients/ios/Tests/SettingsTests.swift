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
    func deviceCodeIsGroupedAsTheOtherDeviceShowsIt(typed: String, shown: String) {
        #expect(DeviceCode.format(typed) == shown)
    }

    @Test func deviceCodeIsCompleteWithTwelveSymbols() {
        #expect(DeviceCode.isComplete("abcd efgh jkmn"))
        #expect(!DeviceCode.isComplete("ABCD-EFGH-JKM"))
        #expect(!DeviceCode.isComplete("ABCD-EFGH-JKM!"))
    }

    @Test func licenseTitlesComeFromTheFileName() throws {
        let url = try #require(URL(string: "file:///Notices/LobeHub-Icons-MIT.txt"))
        #expect(LicenseShelf.title(for: url) == "LobeHub Icons MIT")
        let notice = try #require(URL(string: "file:///Notices/THIRD_PARTY_NOTICES.md"))
        #expect(LicenseShelf.title(for: notice) == "THIRD PARTY NOTICES")
    }

    @Test func licenseProseReflowsAndListsKeepTheirLines() {
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

    @Test func markdownNoticesKeepHeadingsAndInlineCode() {
        #expect(LicenseShelf.block("## Bundled software", markdown: true) == .heading("Bundled software"))
        guard case let .text(text) = LicenseShelf.block("Uses `zlib` here.", markdown: true) else {
            Issue.record("A paragraph is text")
            return
        }
        #expect(String(text.characters) == "Uses zlib here.")
        #expect(LicenseShelf.block("# Not a heading in plain text", markdown: false) == .text(AttributedString("# Not a heading in plain text")))
    }

    @Test func missingNoticesFolderListsNothing() async {
        #expect(await LicenseShelf.files(in: nil).isEmpty)
        #expect(await LicenseShelf.files(in: URL(fileURLWithPath: "/nonexistent/Notices")).isEmpty)
    }
}
