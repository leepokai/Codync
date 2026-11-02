import CodyncKit
import Intents
import SwiftUI
import UserNotifications
import os

/// APNs and the push relay carry generic fallback text; the private title and text arrive
/// sealed to this account's push key (spec §6.7). Opening them is local work only: no network.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        let info = request.content.userInfo
        guard let sealed = info["sealed"] as? String,
              let computerId = info["computerId"] as? String,
              let ctx = info["ctx"] as? String,
              let alert = DeviceIdentity.openPush(sealed: sealed, computerId: computerId, contextID: ctx) else {
            Logger(subsystem: "com.pokai.Codync.ios", category: "NotificationService")
                .error("Showing notification fallback: sealed content or shared key unavailable")
            contentHandler(request.content)
            return
        }
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else {
            contentHandler(request.content)
            return
        }
        content.title = alert.title
        content.subtitle = alert.subtitle ?? ""
        content.body = alert.body
        contentHandler(Self.fromBot(content, alert: alert, botId: info["botId"] as? String, computerId: computerId, ctx: ctx))
    }

    /// A communication notification shows the bot's face in place of the app icon; a group
    /// message shows the group's face with the speaking member's on it. Faces come with the alert,
    /// or (from an older host) from this account's snapshot; without either, or if the system
    /// refuses, the plain notification stays.
    private static func fromBot(_ content: UNMutableNotificationContent, alert: PushAlert, botId: String?, computerId: String, ctx: String) -> UNNotificationContent {
        let store = SharedStore.activeContext
        let snapshot = store.id == ctx ? store.bots.filter { $0.computerId == computerId }.map(\.bot) : []
        let bots = (alert.faces ?? []).map { face in
            var face = face
            face.customIcon = snapshot.first { $0.id == face.id }?.customIcon
            return face
        } + snapshot
        guard let botId, let chat = bots.first(where: { $0.id == botId }) else { return content }
        let members = chat.members.compactMap { id in bots.first { $0.id == id } }
        let conversation = "\(computerId)/\(chat.id)"

        let intent: INSendMessageIntent
        if chat.isGroup, let speaker = bots.first(where: { $0.id == alert.from }) {
            // Two or more recipients make iOS lay it out as a group conversation.
            let me = INPerson(personHandle: INPersonHandle(value: "me", type: .unknown), nameComponents: nil,
                              displayName: nil, image: nil, contactIdentifier: nil, customIdentifier: nil, isMe: true)
            let others = members.filter { $0.id != speaker.id }.map { person($0, computerId: computerId, image: nil) }
            intent = INSendMessageIntent(recipients: [me] + others, outgoingMessageType: .outgoingMessageText, content: content.body,
                                         speakableGroupName: INSpeakableString(spokenPhrase: chat.name), conversationIdentifier: conversation,
                                         serviceName: nil, sender: person(speaker, computerId: computerId, image: avatar(speaker, members: [])), attachments: nil)
            if let face = avatar(chat, members: members) { intent.setImage(face, forParameterNamed: \.speakableGroupName) }
        } else {
            intent = INSendMessageIntent(recipients: nil, outgoingMessageType: .outgoingMessageText, content: content.body,
                                         speakableGroupName: nil, conversationIdentifier: conversation,
                                         serviceName: nil, sender: person(chat, computerId: computerId, image: avatar(chat, members: members)), attachments: nil)
        }
        let interaction = INInteraction(intent: intent, response: nil)
        interaction.direction = .incoming
        interaction.donate(completion: nil)
        return (try? content.updating(from: intent)) ?? content
    }

    private static func person(_ bot: Bot, computerId: String, image: INImage?) -> INPerson {
        INPerson(personHandle: INPersonHandle(value: "\(computerId)/\(bot.id)", type: .unknown), nameComponents: nil,
                 displayName: bot.name, image: image, contactIdentifier: nil, customIdentifier: bot.id)
    }

    /// The bot's avatar, dark like the app icon. SwiftUI draws on the main thread,
    /// and the extension may be called on another one.
    private static func avatar(_ bot: Bot, members: [Bot]) -> INImage? {
        let png = Thread.isMainThread
            ? MainActor.assumeIsolated { render(bot, members: members) }
            : DispatchQueue.main.sync { MainActor.assumeIsolated { render(bot, members: members) } }
        return png.map { INImage(imageData: $0) }
    }

    @MainActor private static func render(_ bot: Bot, members: [Bot]) -> Data? {
        let face = Group {
            if bot.isGroup { GroupAvatar(members: members, size: 88, animated: false) }
            else { CharacterAvatar(bot: bot, size: 88, animated: false) }
        }
        .padding(20)
        .background(Palette.background)
        .environment(\.colorScheme, .dark)
        let renderer = ImageRenderer(content: face)
        renderer.scale = 2
        var png: Data?
        UITraitCollection(userInterfaceStyle: .dark).performAsCurrent { png = renderer.uiImage?.pngData() }
        return png
    }
}
