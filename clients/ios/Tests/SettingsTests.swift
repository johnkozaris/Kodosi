import Foundation
@testable import Kodosi
import KodosiBridge
import SwiftUI
import Testing
import UIKit

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

    @Test func `the lock closes Kodosi only after a minute away`() {
        let left = Date(timeIntervalSince1970: 1000)
        #expect(!AppLock.locks(enabled: false, left: left, now: left.addingTimeInterval(600)))
        #expect(!AppLock.locks(enabled: true, left: nil, now: left))
        #expect(!AppLock.locks(enabled: true, left: left, now: left.addingTimeInterval(59)))
        #expect(AppLock.locks(enabled: true, left: left, now: left.addingTimeInterval(60)))
    }

    @Test func `with the lock on, Kodosi starts locked`() throws {
        let suite = "SettingsTests.lock"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        #expect(!AppLock(defaults: defaults).locked)
        defaults.set(true, forKey: AppLock.storageKey)
        #expect(AppLock(defaults: defaults).locked)
    }

    @Test func `a cancelled unlock asks again only when Kodosi comes back`() async throws {
        let suite = "SettingsTests.cancel"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set(true, forKey: AppLock.storageKey)
        var questions = 0
        let lock = AppLock(defaults: defaults, canProve: { true }, ask: {
            questions += 1
            return .refused
        })
        func settle() async {
            for _ in 0 ..< 20 {
                await Task.yield()
            }
        }
        lock.sceneChanged(to: .active)
        await settle()
        lock.sceneChanged(to: .inactive)
        lock.sceneChanged(to: .active)
        await settle()
        #expect(questions == 1)
        #expect(lock.locked)
        let left = Date()
        lock.sceneChanged(to: .background, now: left)
        lock.sceneChanged(to: .active, now: left.addingTimeInterval(1))
        await settle()
        #expect(questions == 2)
    }

    @Test @MainActor func `the lock covers what Kodosi presents and takes the keys only while it is locked`() throws {
        let scene = try #require(UIApplication.shared.connectedScenes.lazy.compactMap { $0 as? UIWindowScene }.first)
        let main = UIWindow(windowScene: scene)
        main.rootViewController = UIViewController()
        main.makeKeyAndVisible()
        defer { main.isHidden = true }
        let field = UITextField()
        main.addSubview(field)
        let anchor = LockAnchor()
        main.addSubview(anchor)
        anchor.show({ UIViewController() }, locked: false)
        let cover = try #require(anchor.shown)
        #expect(!cover.isHidden && cover.windowLevel > .alert)
        #expect(main.isKeyWindow && field.becomeFirstResponder())
        anchor.show({ UIViewController() }, locked: true)
        #expect(cover.isKeyWindow && !field.isFirstResponder)
        anchor.show(nil, locked: false)
        #expect(cover.isHidden && anchor.shown == nil)
        #expect(main.isKeyWindow)
    }
}
