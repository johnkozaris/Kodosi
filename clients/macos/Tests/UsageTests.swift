import Foundation
import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing

@Test func `limits are named by their window and spoken in full`() throws {
    let answer = """
    {"accounts": [
      {"agent": "claude", "account": "work", "readAtMs": 1792716922000, "limits": [
        {"hours": 5, "scope": null, "model": null, "used": 42, "level": "normal", "resetsAtMs": null},
        {"hours": 168, "scope": null, "model": null, "used": 85, "level": "near", "resetsAtMs": 1795697999513},
        {"hours": 168, "scope": "Fable", "model": "fable", "used": 100, "level": "reached", "resetsAtMs": 1795697999513}]},
      {"agent": "copilot", "account": "default", "readAtMs": 1792716922000, "limits": [
        {"hours": null, "scope": null, "model": null, "used": 59, "level": "normal", "resetsAtMs": null}]}]}
    """
    let reply = try JSONDecoder().decode(AgentUsageReply.self, from: Data(answer.utf8))
    let claude = try #require(reply.accounts.first)
    #expect(claude.limits.map(\.window) == ["5h", "week", "Fable"])
    #expect(claude.limits.map(\.id) == ["hours:5", "hours:168", "scope:Fable"])
    #expect(claude.limits.map(\.spoken) == [
        "Last 5 hours: 42% used", "This week: 85% used", "Fable, this week: 100% used",
    ])
    #expect(claude.limits[1].resetsAt == Date(timeIntervalSince1970: 1_795_697_999.513))
    let copilot = try #require(reply.accounts.last?.limits.first)
    #expect(copilot.window == "month")
    #expect(copilot.spoken == "This month: 59% used")
}

private func accounts(_ json: String) throws -> [AccountUsage] {
    try JSONDecoder().decode(AgentUsageReply.self, from: Data(#"{"accounts": \#(json)}"#.utf8)).accounts
}

private func limit(_ json: String) throws -> UsageLimit {
    try JSONDecoder().decode(UsageLimit.self, from: Data(json.utf8))
}

private let session = #"{"hours": 5, "scope": null, "model": null, "used": 1, "level": "normal", "resetsAtMs": null}"#
private let week = #"{"hours": 168, "scope": null, "model": null, "used": 91, "level": "near", "resetsAtMs": null}"#
private let fable = #"{"hours": 168, "scope": "Fable", "model": "fable", "used": 100, "level": "reached", "resetsAtMs": null}"#
private let month = #"{"hours": null, "scope": null, "model": null, "used": 3, "level": "normal", "resetsAtMs": null}"#

@Test func `the headline of an account counts only the limits of the model that starts`() throws {
    let both = try accounts(#"""
    [{"agent": "claude", "account": "default", "readAtMs": 0, "limits": [\#(session), \#(week), \#(fable)]},
     {"agent": "claude", "account": "alt", "readAtMs": 0, "limits": [\#(fable)]}]
    """#)
    let usage = try #require(both.first)
    #expect(usage.headline(for: "opus")?.id == "hours:168")
    #expect(usage.headline(for: "fable")?.id == "scope:Fable")
    #expect(usage.headline(for: nil)?.id == "scope:Fable")
    #expect(both.last?.headline(for: "opus") == nil)
}

@Test func `the nearest limit has the worst level, then the most use`() throws {
    let normal = try limit(#"{"hours": 5, "used": 79, "level": "normal"}"#)
    let near = try limit(#"{"hours": 5, "used": 70, "level": "near"}"#)
    #expect(UsageLimit.nearest(of: [normal, near]) == near)
    let full = try limit(#"{"hours": 5, "used": 100, "level": "near"}"#)
    let reached = try limit(#"{"hours": 5, "used": 100, "level": "reached"}"#)
    #expect(UsageLimit.nearest(of: [full, reached]) == reached)
    let shorter = try limit(#"{"hours": 5, "used": 30, "level": "normal"}"#)
    let longer = try limit(#"{"hours": 168, "used": 30, "level": "normal"}"#)
    #expect(UsageLimit.nearest(of: [shorter, longer]) == shorter)
    #expect(UsageLimit.nearest(of: []) == nil)
}

@Test func `the panel orders the windows of all accounts from the shortest`() throws {
    let both = try accounts(#"""
    [{"agent": "claude", "account": "default", "readAtMs": 0, "limits": [\#(week), \#(fable)]},
     {"agent": "claude", "account": "alt", "readAtMs": 0, "limits": [\#(month), \#(session), \#(week), \#(fable)]}]
    """#)
    #expect(UsagePanel.windows(of: both).map(\.id) == ["hours:5", "hours:168", "scope:Fable", "month"])
}

@Test func `a reset shows the time left in its two largest units, rounded up`() throws {
    let english = Locale(identifier: "en_US")
    let short = { (seconds: TimeInterval) in ResetTime.short(seconds, locale: english) }
    #expect(short(4 * 86400 + 45 * 60) == "4d 1h")
    #expect(short(5 * 86400) == "5d")
    #expect(short(4 * 3600 + 47 * 60 + 1) == "4h 48m")
    #expect(short(30) == "1m")
    #expect(short(59 * 60 + 30) == "1h")
    #expect(short(23 * 3600 + 59 * 60 + 30) == "1d")
    #expect(ResetTime.spoken(4 * 3600 + 48 * 60, locale: english) == "4 hours, 48 minutes")
    let now = Date(timeIntervalSince1970: 1_795_349_410)
    let resets = try limit(#"{"hours": 5, "scope": null, "model": null, "used": 1, "level": "normal", "resetsAtMs": 1795349470000}"#)
    #expect(ResetTime.left(resets, now: now) == 60)
    #expect(ResetTime.left(resets, now: now.addingTimeInterval(61)) == nil)
}
