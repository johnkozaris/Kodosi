import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test func `a new task names the repository of its issue only when it opens one`() {
    let plain = ProjectAction.createTask(title: " Fix it ", description: "", repositoryIds: ["b", "a"])
    #expect(plain["issueRepositoryId"] == nil)
    #expect(plain["repositoryIds"] == .array([.string("a"), .string("b")]))
    let issue = ProjectAction.createTask(title: "Fix it", description: "", repositoryIds: [], issueRepositoryId: "a")
    #expect(issue["issueRepositoryId"] == .string("a"))
}

@Test func `an issue number in the search finds an issue that the list does not show`() {
    #expect(IssuePicker.number(in: "#482") == 482)
    #expect(IssuePicker.number(in: "17") == 17)
    #expect(IssuePicker.number(in: "#0") == nil)
    #expect(IssuePicker.number(in: "fix 17") == nil)
    #expect(IssuePicker.number(in: "#") == nil)
    #expect(IssuePicker.number(in: "") == nil)
}
