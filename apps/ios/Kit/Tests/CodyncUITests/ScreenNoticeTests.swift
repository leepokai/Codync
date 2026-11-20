import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A screen that can't show says why, in the order Android and the desktop check: a sleeping cloud
/// computer is waking (connecting wakes it) before anything reads as offline.
@MainActor @Test func screenNoticesComeInTheOrderTheOtherAppsCheck() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in FakeRemote(.connecting) }
    defer { store.retire() }
    func notice() -> (icon: String?, text: String)? { ScreenNotice(store).map { ($0.icon, $0.text) } }

    store.connection = .computerOffline(lastSeen: nil)
    store.cloudHosting = CloudHosting(provider: "boat", providerName: "Boat", region: "EU", state: .stopped)
    #expect(notice()?.text == "Waking the computer…" && notice()?.icon == nil)
    store.cloudHosting = nil
    #expect(notice()?.icon == "wifi.slash")
    store.connection = .connecting
    #expect(notice()?.text == "Connecting to the screen…")
    store.connection = .online
    #expect(notice()?.icon == "arrow.down.circle", "a host too old to report its screen")
    var screen = ScreenState()
    store.screen = screen
    #expect(notice()?.icon == "lock.display")
    (screen.enabled, screen.connected, screen.capture) = (true, true, true)
    store.screen = screen
    #expect(notice() == nil, "the screen can show")
}
