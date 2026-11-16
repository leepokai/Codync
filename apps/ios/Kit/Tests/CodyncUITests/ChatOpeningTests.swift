import CryptoKit
import Foundation
import SwiftUI
import Testing
import UIKit
@testable import CodyncKit
@testable import CodyncUI

@Test func onlyVisibleConversationItemsFinishOpening() {
    let rows = [
        ConversationRow("intro", isPlaceholder: true) { EmptyView() },
        ConversationRow("working", isPlaceholder: true) { EmptyView() },
        ConversationRow("message") { EmptyView() },
    ]
    #expect(!hasLaidOutMessage(rows, visibleIDs: ["intro", "working"]))
    #expect(hasLaidOutMessage(rows, visibleIDs: ["intro", "message"]))
}

@MainActor @Test func placeholderOnlyLayoutWaitsForAMessageToFinishOpening() {
    let view = ConversationCollectionView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                          collectionViewLayout: UICollectionViewFlowLayout())
    var opened = 0
    view.onOpened = { opened += 1 }
    view.layoutIfNeeded()
    #expect(opened == 0)
    view.hasLaidOutMessage = { true }
    view.setNeedsLayout()
    view.layoutIfNeeded()
    #expect(opened == 1)
    view.setNeedsLayout()
    view.layoutIfNeeded()
    #expect(opened == 1)
}

@MainActor @Test func theSelectionActionStartsTheChatOpeningAndItsFirstMessagesFinishItOnce() throws {
    let suite = "ChatOpening.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: nil, suite: suite)
    let key = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
    storage.computers = [Computer(id: RelayCrypto.computerId(signKey: key), name: "Mac", signKey: key.base64URL)]
    let accounts = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
    }
    defer { accounts.retire() }
    let computer = try #require(accounts.computers.first)
    let store = try #require(accounts.store(for: computer.id))
    let ref = BotReference(accountId: nil, computerId: computer.id, botId: "b1")

    accounts.selection = ref
    #expect(store.chatOpening.finish("b1") != nil)
    #expect(store.chatOpening.finish("b1") == nil)

    accounts.selection = ref
    #expect(store.chatOpening.finish("b1") == nil)
    accounts.selection = nil
    accounts.selection = ref
    accounts.selection = nil
    #expect(store.chatOpening.finish("b1") == nil)
}
