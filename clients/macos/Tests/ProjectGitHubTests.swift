@testable import KodosiDesktop
import Testing

@Test func `an issue number in the search finds an issue that the list does not show`() {
    #expect(IssuePicker.number(in: "#482") == 482)
    #expect(IssuePicker.number(in: "17") == 17)
    #expect(IssuePicker.number(in: "#0") == nil)
    #expect(IssuePicker.number(in: "fix 17") == nil)
    #expect(IssuePicker.number(in: "#") == nil)
    #expect(IssuePicker.number(in: "") == nil)
}
