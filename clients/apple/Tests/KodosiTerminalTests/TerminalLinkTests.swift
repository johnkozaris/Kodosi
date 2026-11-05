import Foundation
import KodosiLinks
@testable import KodosiTerminal
import Testing

@MainActor
struct TerminalLinkTests {
    @Test func `the proxy opens a safe link and lets the view review every other link`() throws {
        let proxy = TerminalRendererDelegateProxy()
        var opened: [URL] = []
        var reviewed: [ClickedLink] = []
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

        proxy.terminalDidRequestOpenURL("http://localhost:5173/", kind: .unknown)
        #expect(try opened.last == #require(URL(string: "http://localhost:5173/")))
        proxy.otherComputer = "studio"
        proxy.terminalDidRequestOpenURL("http://localhost:5173/", kind: .unknown)
        #expect(reviewed.last?.decision == .refuse(.addressElsewhere("studio")))
        proxy.terminalDidRequestOpenURL("/private/tmp/screen.txt", kind: .text)
        #expect(opened.last == URL(filePath: "/private/tmp/screen.txt"))
        #expect(reviewed.count == 3)
    }
}
