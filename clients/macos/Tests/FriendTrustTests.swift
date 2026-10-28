import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test func `the trust warning names the person of a username or an invite`() {
    #expect(FriendTrust.add("maya").name == "@maya")
    #expect(FriendTrust.add(" @maya.k ").handle == "maya.k")
    #expect(FriendTrust.add("kodosi:maya:\(String(repeating: "ab", count: 32))").handle == "maya")
    #expect(FriendTrust.add("maya smith").name == nil)
    #expect(FriendTrust.add("https://example.com/maya").handle == nil)
}
