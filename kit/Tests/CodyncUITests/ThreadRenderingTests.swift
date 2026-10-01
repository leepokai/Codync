#if os(macOS)
import AppKit
import Observation
import SwiftUI
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Observable
private final class RenderSelection {
    var botId = "long"
}

private struct RenderConversation: View {
    let selection: RenderSelection
    let store: BotStore

    var body: some View {
        ThreadView(botId: selection.botId)
            .environment(store)
            .id(selection.botId)
            .transition(.asymmetric(insertion: .opacity, removal: .identity))
            .animation(Motion.fade, value: selection.botId)
    }
}

@MainActor @Test func conversationShowsMessagesWhenSwitchingWithoutScrolling() async throws {
    let suite = "ThreadRenderingTests.\(UUID())"
    let storage = SharedStore.Context(accountID: suite, suite: suite)
    defer { storage.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let computer = Computer(id: "render-test", name: "Fixture", signKey: "")
    let store = BotStore(computer: computer, route: .channel, clientKind: "macos", storage: storage) { fake }
    defer { store.retire() }
    store.setActive(true)
    for _ in 0..<200 {
        let subscribed = await fake.subscribed
        if subscribed { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    for (botId, count) in [("long", 200), ("short", 2)] {
        await fake.emit(#"{"type":"bot","bot":{"id":"\#(botId)","name":"\#(botId)","rev":1,"lastAt":1}}"#)
        for index in 1...count {
            let text = "Visible \(botId) message \(index). " + String(repeating: "Content of the conversation. ", count: index % 20 + 1)
            let entry = Entry(id: "\(botId)-\(index)", seq: Int64(index), botId: botId, rev: Int64(index), kind: "agent", turn: Int64(index), data: EntryData(text: text), createdAt: 1, updatedAt: 1)
            var finalEntry = entry
            finalEntry.data.final = true
            let encoded = try JSONEncoder().encode(finalEntry)
            let json = String(decoding: encoded, as: UTF8.self)
            await fake.emit("{\"type\":\"entry\",\"entry\":\(json)}")
        }
    }
    for _ in 0..<200 {
        if store.chat("long").count == 200 && store.chat("short").count == 2 { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(store.chat("long").count == 200)
    #expect(store.chat("short").count == 2)

    _ = NSApplication.shared
    let selection = RenderSelection()
    let hosting = NSHostingView(rootView: RenderConversation(selection: selection, store: store))
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1100, height: 900), styleMask: [.borderless], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: .aqua)
    window.contentView = hosting
    defer { window.close() }

    // Match the Mac sidebar's identity replacement and fade, including a narrow
    // window that hides the inspector. No mouse wheel or scroll request is sent.
    for (botId, width) in [("long", 1100), ("short", 1100), ("long", 620), ("short", 620), ("long", 1100), ("short", 1100)] {
        selection.botId = botId
        window.setContentSize(NSSize(width: width, height: 900))
        for _ in 0..<30 {
            hosting.layoutSubtreeIfNeeded()
            hosting.displayIfNeeded()
            try await Task.sleep(for: .milliseconds(10))
        }
        let bitmap = try #require(hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds))
        hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
        #expect(transcriptTextPixels(bitmap, viewSize: hosting.bounds.size) > 100)
    }
}

/// Only the message area counts: exclude the header, composer and inspector so
/// populated surrounding chrome cannot conceal a blank conversation regression.
@MainActor private func transcriptTextPixels(_ bitmap: NSBitmapImageRep, viewSize: CGSize) -> Int {
    let scale = CGFloat(bitmap.pixelsWide) / viewSize.width
    let transcriptWidth = viewSize.width - (viewSize.width >= 680 ? 293 : 0)
    let right = Int((transcriptWidth - 16) * scale)
    let bottom = Int((viewSize.height - 90) * scale)
    var count = 0
    for y in stride(from: Int(70 * scale), to: bottom, by: 3) {
        for x in stride(from: Int(16 * scale), to: right, by: 3) {
            guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else { continue }
            if color.alphaComponent > 0.9 && color.redComponent < 0.5
                && color.greenComponent < 0.5 && color.blueComponent < 0.5 {
                count += 1
            }
        }
    }
    return count
}
#endif
