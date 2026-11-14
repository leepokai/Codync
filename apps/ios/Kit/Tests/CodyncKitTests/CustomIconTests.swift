import CodyncKit
import Foundation
import Testing

@Test func customIconsUseStaticLocalPngNames() throws {
    let icon = try JSONDecoder().decode(CustomIcon.self, from: Data(#"{"dataUrl":"data:image/png;base64,iVBORw0KGgo=","tinted":false}"#.utf8))
    #expect(icon.pngData?.count == 8)
    #expect(icon.filename?.count == 68)
    #expect(icon.filename?.hasSuffix(".png") == true)
    var bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b"}"#.utf8))
    #expect(BotActivityAttributes(bot: bot, computerId: "test").customIconName == nil)
    bot.customIcon = icon
    let attributes = BotActivityAttributes(bot: bot, computerId: "test")
    #expect(attributes.customIconName == icon.filename)
    #expect(attributes.customIconTinted == false)
    #expect(try JSONEncoder().encode(attributes).count < 1024)
}

@Test func remoteAndSvgDataAreNotRenderedAsCustomIcons() throws {
    for value in ["https://example.com/image.png", "data:image/svg+xml,<svg/>", "data:image/png;base64,abcd"] {
        let data = try JSONSerialization.data(withJSONObject: ["dataUrl": value, "tinted": false])
        let icon = try JSONDecoder().decode(CustomIcon.self, from: data)
        #expect(icon.pngData == nil)
    }
}

@Test func iconEventsRetainColorMode() throws {
    let data = Data(#"{"type":"icons","icons":{"bot:b":{"dataUrl":"data:image/png;base64,iVBORw0KGgo=","tinted":true}}}"#.utf8)
    guard case let .icons(icons, _) = HostClient.parseEvent(data) else { Issue.record("Expected icons event"); return }
    #expect(icons["bot:b"]?.tinted == true)
}
