import Foundation
import SwiftUI
import Testing
@testable import CodyncKit

/// Issue #549: a group room is not a bot, so the bots widgets' "N bots" count leaves rooms out.
private func member(_ id: String, kind: String = "bot") throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"\#(id)","name":"\#(id)","kind":"\#(kind)"}"#.utf8))
}

@MainActor private func pixels(_ view: some View) throws -> Data {
    let renderer = ImageRenderer(content: view.frame(width: 160, height: 80))
    renderer.scale = 2
    return try #require(renderer.uiImage?.pngData())
}

@MainActor @Test(arguments: AccessoryWidgetCard.Family.allCases.map(\.rawValue))
func botsWidgetCountLeavesGroupRoomsOut(familyName: String) throws {
    let family = try #require(AccessoryWidgetCard.Family(rawValue: familyName))
    let bots = try (1...7).map { try member("b\($0)") }
    let room = try member("room", kind: "group")
    #expect(try pixels(AccessoryWidgetCard(kind: .bots, family: family, bots: bots + [room]))
        == pixels(AccessoryWidgetCard(kind: .bots, family: family, bots: bots)),
        "7 bots and a room read as 7 bots, not 8")
}
