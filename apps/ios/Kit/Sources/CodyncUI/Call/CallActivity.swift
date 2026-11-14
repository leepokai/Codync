import ActivityKit
import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// The bot's Live Activity while this phone is on a call with it: the call's state and length, and the bot's latest
/// progress from the computer. Uses the bot's activity if one is up (a task it was given), else starts one for the call.
/// The call's fields live here: every update of the activity merges them in (`withCall`), and they go back on after
/// the computer's pushes, which replace its state without them.
@MainActor
public final class CallActivity {
    public typealias State = BotActivityAttributes.ContentState

    /// The calls this phone is on, by bot.
    private static var calls: [String: CallActivity] = [:]

    private let attributes: BotActivityAttributes
    private let startedAt = Date.now
    /// The activity this call started itself (none was up): the computer can't update it.
    private var ownID: String?
    private var call: CallState?
    /// The computer's latest progress line, until the bot's reply.
    private var progress: String?
    private var muted = false
    /// The call's own mute and hang-up, which the activity's buttons press (`MuteCallIntent`, `EndCallIntent`).
    private let toggleMute: @MainActor () -> Void
    private let hangUp: @MainActor () -> Void
    /// The activity followed for the computer's updates, and the tasks following it.
    private var followed: (id: String, tasks: [Task<Void, Never>])?

    init(bot: Bot, computerId: ComputerID, link: URL?, toggleMute: @escaping @MainActor () -> Void = {},
         hangUp: @escaping @MainActor () -> Void = {}) {
        attributes = BotActivityAttributes(bot: bot, computerId: computerId, link: link)
        do { try bot.customIcon?.cacheForActivity(context: attributes.iconContext) }
        catch { log.error("Cannot cache call icon: \(error.localizedDescription)") }
        self.toggleMute = toggleMute
        self.hangUp = hangUp
        Self.calls[bot.id] = self
    }

    /// The activity's mute button: the call's microphone goes off or back on, as with the call bar's.
    public static func toggleMute(botId: String) { calls[botId]?.toggleMute() }

    /// The activity's end button: the call ends as with the call bar's.
    public static func hangUp(botId: String) { calls[botId]?.hangUp() }

    /// Whether this phone is on a call with the bot.
    public static func isLive(botId: String) -> Bool { calls[botId] != nil }

    /// Whether a call started this activity itself.
    public static func started(_ activity: Activity<BotActivityAttributes>) -> Bool {
        calls[activity.attributes.botId]?.ownID == activity.id
    }

    /// `state` with the fields of the call this phone is on with the bot, or with none.
    public static func withCall(_ state: State, botId: String) -> State {
        let live = calls[botId]
        return merge(state, call: live?.call, since: live?.startedAt, muted: live?.muted == true)
    }

    /// `ContentState.call` for a phase: nil once the call can't go on.
    static func state(_ phase: VoicePhase) -> CallState? {
        switch phase {
        case .starting, .reconnecting: .connecting
        case .listening: .listening
        case .speaking: .speaking
        case .interrupted: .paused
        case .failed: nil
        }
    }

    func phase(_ phase: VoicePhase) {
        let state = Self.state(phase)
        guard state != call else { return }
        call = state
        show()
    }

    /// The call's microphone went off or on (its call bar or the activity's button).
    func muted(_ on: Bool) {
        guard on != muted else { return }
        muted = on
        show()
    }

    /// What the computer said on the call: its progress shows until the bot's reply.
    func heard(_ cue: CallCue) {
        switch cue.kind {
        case .progress:
            progress = cue.text
            show(activity: cue.text)
        case .reply: progress = nil
        case .notice: break
        }
    }

    func end() {
        call = nil
        followed?.tasks.forEach { $0.cancel() }
        followed = nil
        // A newer call with the bot keeps the activity.
        guard Self.calls[attributes.botId] === self else { return }
        Self.calls[attributes.botId] = nil
        let (botId, computerId, own) = (attributes.botId, attributes.computerId, ownID)
        Task.detached {
            guard let activity = BotActivityAttributes.shown(botId: botId, computerId: computerId) else { return }
            await Self.callEnded(on: activity, dismissNow: activity.id == own)
        }
    }

    /// A call is over on `activity`: one whose bot has nothing left to show ends (at once with `dismissNow`), and one
    /// with a task under way goes on without the call.
    public nonisolated static func callEnded(on activity: Activity<BotActivityAttributes>, dismissNow: Bool) async {
        let state = merge(activity.content.state, call: nil, since: nil)
        if ["idle", "completed", "error"].contains(state.status) {
            await activity.end(.init(state: state, staleDate: nil), dismissalPolicy: dismissNow ? .immediate : .after(.now + 60))
        } else {
            await activity.update(.init(state: state, staleDate: .now + 15 * 60))
        }
    }

    nonisolated static func merge(_ state: State, call: CallState?, since: Date?, muted: Bool = false) -> State {
        var state = state
        state.call = call
        state.callStartedAt = call == nil ? nil : since
        state.callMuted = call == nil || !muted ? nil : true
        return state
    }

    /// Puts the call on the bot's activity, with `activity` as its line if given, or starts one for the call.
    private func show(activity text: String? = nil) {
        let (botId, computerId) = (attributes.botId, attributes.computerId)
        guard Self.calls[botId] === self else { return }
        guard let shown = BotActivityAttributes.shown(botId: botId, computerId: computerId) else { return start() }
        follow(shown)
        let (call, startedAt, muted) = (call, startedAt, muted)
        Task.detached {
            guard let activity = BotActivityAttributes.shown(botId: botId, computerId: computerId) else { return }
            var state = Self.merge(activity.content.state, call: call, since: startedAt, muted: muted)
            if let text { state.activity = text }
            guard state != activity.content.state else { return }
            await activity.update(.init(state: state, staleDate: activity.content.staleDate))
        }
    }

    private func start() {
        guard call != nil, BotActivityAttributes.mayStart else { return }
        let state = Self.merge(.init(status: "idle", activity: progress ?? "", startedAt: nil), call: call, since: startedAt,
                               muted: muted)
        do {
            let activity = try Activity.request(attributes: attributes, content: .init(state: state, staleDate: nil), pushType: nil)
            ownID = activity.id
            follow(activity)
        } catch {
            log.error("Live activity for the call not started: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Follows the activity showing the call: a push from the computer replaces its state without the call's fields,
    /// which go back on, and its last one ends it, which gives the call its own activity (one the user swiped away
    /// stays away).
    private func follow(_ activity: Activity<BotActivityAttributes>) {
        guard followed?.id != activity.id else { return }
        followed?.tasks.forEach { $0.cancel() }
        followed = (activity.id, [
            Task { [weak self] in for await _ in activity.contentUpdates { self?.show() } },
            Task { [weak self] in for await state in activity.activityStateUpdates where state == .ended { self?.show() } },
        ])
    }
}
