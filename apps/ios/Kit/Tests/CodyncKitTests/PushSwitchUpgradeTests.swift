import Foundation
import Testing
@testable import CodyncKit

/// 2.28.0's one push switch becomes each saved computer's own switch, once.
struct PushSwitchUpgradeTests {
    private let suite = "CodyncKitTests.\(UUID().uuidString)"
    private var defaults: UserDefaults { UserDefaults(suiteName: suite)! }

    private func context(_ accountID: String?) -> SharedStore.Context {
        let context = SharedStore.Context(accountID: accountID, suite: suite)
        context.computers = [Computer(id: "\(accountID ?? "local")-desk", name: "Desk", signKey: "key")]
        return context
    }

    @Test func anOldSwitchTurnedOffTurnsOffEveryComputerAndIsRemoved() {
        defer { defaults.removePersistentDomain(forName: suite) }
        let contexts = [context(nil), context("user_a")]
        defaults.set(false, forKey: "notificationsEnabled")
        SharedStore.Context.splitPushSwitch(from: defaults, into: contexts)
        #expect(contexts.allSatisfy { $0.computers.allSatisfy { $0.notifications == false } })
        #expect(defaults.object(forKey: "notificationsEnabled") == nil)
    }

    @Test func anOldSwitchLeftOnOrNeverSetChangesNothing() {
        defer { defaults.removePersistentDomain(forName: suite) }
        let contexts = [context(nil)]
        defaults.set(true, forKey: "notificationsEnabled")
        SharedStore.Context.splitPushSwitch(from: defaults, into: contexts)
        SharedStore.Context.splitPushSwitch(from: defaults, into: contexts)
        #expect(contexts[0].computers.first?.notifications == nil)
        #expect(defaults.object(forKey: "notificationsEnabled") == nil)
    }

    @Test func aLaterLaunchKeepsWhatThePhoneChoseSince() {
        defer { defaults.removePersistentDomain(forName: suite) }
        let contexts = [context(nil)]
        defaults.set(false, forKey: "notificationsEnabled")
        SharedStore.Context.splitPushSwitch(from: defaults, into: contexts)
        contexts[0].computers = contexts[0].computers.map { var computer = $0; computer.notifications = nil; return computer }
        SharedStore.Context.splitPushSwitch(from: defaults, into: contexts)
        #expect(contexts[0].computers.first?.notifications == nil)
    }
}
