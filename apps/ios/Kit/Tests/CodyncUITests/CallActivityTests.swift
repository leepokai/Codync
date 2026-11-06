import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func theLiveActivityFollowsTheCallsPhase() {
    #expect(CallActivity.state(.starting) == .connecting)
    #expect(CallActivity.state(.reconnecting) == .connecting)
    #expect(CallActivity.state(.listening) == .listening)
    #expect(CallActivity.state(.speaking) == .speaking)
    #expect(CallActivity.state(.interrupted) == .paused)
    #expect(CallActivity.state(.failed("x", .connection)) == nil)
}

@MainActor @Test func anActivityShowsTheCallOnlyWhileThisPhoneIsOnIt() throws {
    var bot = try #require(Bot.widgetPreview.first)
    bot.id = "call-activity-test"
    // The computer's push carries no call; one left on the activity by a call that's over goes.
    let pushed = BotActivityAttributes.ContentState(status: "working", activity: "", startedAt: nil)
    let leftover = CallActivity.merge(pushed, call: .listening, since: .now)
    #expect(CallActivity.withCall(leftover, botId: bot.id) == pushed)

    let older = CallActivity(bot: bot, computerId: "computer", link: nil)
    older.phase(.speaking)
    let shown = CallActivity.withCall(pushed, botId: bot.id)
    #expect(shown.call == .speaking && shown.callStartedAt != nil && shown.status == "working")
    // A newer call with the bot outlives the older one ending.
    let newer = CallActivity(bot: bot, computerId: "computer", link: nil)
    newer.phase(.listening)
    older.end()
    #expect(CallActivity.isLive(botId: bot.id))
    #expect(CallActivity.withCall(pushed, botId: bot.id).call == .listening)
    newer.end()
    #expect(!CallActivity.isLive(botId: bot.id))
    #expect(CallActivity.withCall(leftover, botId: bot.id) == pushed)
}

@MainActor @Test func theActivitysButtonsReachTheCallAndShowItsMute() throws {
    var bot = try #require(Bot.widgetPreview.first)
    bot.id = "call-activity-buttons"
    var mutes = 0, hangUps = 0
    let call = CallActivity(bot: bot, computerId: "computer", link: nil, toggleMute: { mutes += 1 }, hangUp: { hangUps += 1 })
    call.phase(.listening)
    CallActivity.toggleMute(botId: bot.id)
    CallActivity.hangUp(botId: bot.id)
    CallActivity.toggleMute(botId: "another-bot")
    #expect(mutes == 1 && hangUps == 1)

    let pushed = BotActivityAttributes.ContentState(status: "idle", activity: "", startedAt: nil)
    #expect(CallActivity.withCall(pushed, botId: bot.id).callMuted == nil)
    call.muted(true)
    #expect(CallActivity.withCall(pushed, botId: bot.id).callMuted == true)
    call.end()
    // A call that's over leaves no mute behind, and its buttons do nothing.
    #expect(CallActivity.withCall(pushed, botId: bot.id).callMuted == nil)
    CallActivity.hangUp(botId: bot.id)
    #expect(hangUps == 1)
}
