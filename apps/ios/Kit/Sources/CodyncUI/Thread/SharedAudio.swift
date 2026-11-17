import AVFoundation
import CodyncKit
import Foundation
import Observation

/// Plays a sound a bot shared (a voice note), fetched from the computer; one player per file card or
/// voice-note bubble.
@MainActor
@Observable
final class SharedAudio {
    enum State {
        /// Never played, or played to its end / stopped with `elapsed` back at 0.
        case idle, loading, playing, paused, failed
    }
    private(set) var state = State.idle
    /// Seconds the sound runs, known once its first load decoded.
    private(set) var duration: TimeInterval?
    /// Seconds into the sound; ticks about four times a second while it plays.
    private(set) var elapsed: TimeInterval = 0
    @ObservationIgnored private var player: AVAudioPlayer?
    @ObservationIgnored private var done: PlayDone?
    /// Whether this player turned the audio session on (a call or dictation keeps its own).
    @ObservationIgnored private var ownsSession = false
    /// Bumped by `stop()`: a load still running when the card went doesn't start playing.
    @ObservationIgnored private var generation = 0
    /// Ticks `elapsed` while the sound plays.
    @ObservationIgnored private var clock: Timer?
    /// A call or Siri taking the audio pauses the note, as the user's own pause would.
    @ObservationIgnored private var interruptions: AudioSessionEvents?
    /// The note now sounding: starting another pauses it first, so only one owns the audio session.
    private static weak var sounding: SharedAudio?
    /// The step this tap is on, so a failure says which one broke.
    private(set) var failure = AnalyticsEvent.VoiceNoteFailure.load
    /// Fetches the sound's bytes; tests hold it open to stop the card mid-load.
    private let load: @MainActor (BotStore, SharedFile, String) async -> Data?
    /// Hands the audio back to other apps; tests count it.
    private let deactivate: @MainActor () -> Void

    init(load: @escaping @MainActor (BotStore, SharedFile, String) async -> Data? = { await $0.sharedFileData($1, entryId: $2) },
         deactivate: @escaping @MainActor () -> Void = CallAudioSession.releasePlayback) {
        self.load = load
        self.deactivate = deactivate
    }

    /// The card's one control: a tap while the sound loads is dropped, so it never starts a second player.
    func toggle(_ file: SharedFile, entryId: String, model: BotStore) async {
        let fromStart = state == .idle || state == .failed
        switch state {
        case .loading: return
        case .playing:
            pause()
            return
        case .idle, .failed:
            state = .loading
        case .paused:
            break
        }
        let started = generation
        failure = .load
        do {
            let player = try await prepared(file, entryId: entryId, model: model, for: started)
            failure = .playback
            if let other = Self.sounding, other !== self, other.state == .playing { other.pause() }
            if try CallAudioSession.activatePlayback() { ownsSession = true }
            guard player.play() else { throw CocoaError(.coderReadCorrupt) }
            Self.sounding = self
            elapsed = player.currentTime
            state = .playing
            startClock()
            interruptions = interruptions ?? AudioSessionEvents(engine: nil) { [weak self] interruption in
                if case .began = interruption, self?.state == .playing { self?.pause() }
            }
            if fromStart { Analytics.shared.capture(.voiceNotePlayed(nil)) }
        } catch {
            // A card that went away mid-load never shows the failure.
            guard generation == started else { return }
            fail()
            if fromStart { Analytics.shared.capture(.voiceNotePlayed(failure)) }
        }
    }

    func stop() {
        generation += 1
        player?.stop()
        player?.currentTime = 0
        interruptions?.stop()
        interruptions = nil
        finish()
    }

    /// Moves a loaded sound's playhead, held to its length; playing or paused, it stays so.
    func seek(to seconds: TimeInterval) {
        guard let player, let duration else { return }
        player.currentTime = min(max(seconds, 0), duration)
        elapsed = player.currentTime
    }

    private func pause() {
        guard player != nil else { state = .idle; return }
        elapsed = player?.currentTime ?? 0
        player?.pause()
        stopClock()
        state = .paused
        releaseSession()
    }

    /// Stopped or played to its end: back to the start, its length still known so the card keeps showing it.
    private func finish() {
        elapsed = 0
        stopClock()
        state = .idle
        releaseSession()
    }

    /// The sound or the session failed: the player is dropped so the next tap fetches and decodes again.
    private func fail() {
        (player, done) = (nil, nil)
        duration = nil
        elapsed = 0
        stopClock()
        state = .failed
        releaseSession()
    }

    /// Only the session this note activated is ours to release; a recording started since keeps it.
    private func releaseSession() {
        guard ownsSession else { return }
        ownsSession = false
        guard !CallAudioSession.recording else { return }
        deactivate()
    }

    /// The tap's player, loaded if it has none; a load the card outlived (`stop()` since `started`) throws before it
    /// changes anything, so a newer tap's player and state stay.
    private func prepared(_ file: SharedFile, entryId: String, model: BotStore, for started: Int) async throws -> AVAudioPlayer {
        if let player { return player }
        let data = await load(model, file, entryId)
        guard generation == started else { throw CancellationError() }
        guard let data else { throw CocoaError(.fileReadUnknown) }
        let player = try AVAudioPlayer(data: data)
        duration = player.duration
        let done = PlayDone { [weak self] played in Task { @MainActor in played ? self?.finish() : self?.fail() } }
        player.delegate = done
        (self.player, self.done) = (player, done)
        return player
    }

    /// About four ticks a second, reading the player's own clock.
    private func startClock() {
        stopClock()
        clock = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self, let player else { return }
                elapsed = player.currentTime
            }
        }
    }

    private func stopClock() {
        clock?.invalidate()
        clock = nil
    }

    /// `0:07` under an hour, `1:02:48` past it.
    static func timeText(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds))
        let (m, s) = (total / 60 % 60, total % 60)
        let h = total / 3600
        return h > 0 ? "\(h):" + String(format: "%02d:%02d", m, s) : "\(m):" + String(format: "%02d", s)
    }
}
