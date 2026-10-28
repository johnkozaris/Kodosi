import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test func `the repository list of a computer reads its GitHub state and choices`() throws {
    let data = Data(#"""
    {"github":"signedOut","repositories":[
      {"name":"team/api","url":"https://github.com/team/api","provider":"github","folder":"/Users/maya/api","private":false},
      {"name":"team/web","url":"https://github.com/team/web","provider":"github","folder":null,"private":true}
    ]}
    """#.utf8)
    let choices = try JSONDecoder().decode(RepositoryChoices.self, from: data)
    #expect(choices.github == .signedOut)
    #expect(choices.repositories.map(\.folder) == ["/Users/maya/api", nil])
    #expect(choices.repositories[1].private)
}

@Test func `a new task names the repository of its issue only when it opens one`() {
    let plain = RoomAction.createTask(title: " Fix it ", description: "", repositoryIds: ["b", "a"])
    #expect(plain["issueRepositoryId"] == nil)
    #expect(plain["repositoryIds"] == .array([.string("a"), .string("b")]))
    let issue = RoomAction.createTask(title: "Fix it", description: "", repositoryIds: [], issueRepositoryId: "a")
    #expect(issue["issueRepositoryId"] == .string("a"))
}
