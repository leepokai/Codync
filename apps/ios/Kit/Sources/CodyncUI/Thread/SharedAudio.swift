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
    /// Fetches the sound's bytes; tests hold it open to stop the card mid-load.
    private let load: @MainActor (BotStore, SharedFile, String) async -> Data?

    init(load: @escaping @MainActor (BotStore, SharedFile, String) async -> Data? = { await $0.sharedFileData($1, entryId: $2) }) {
        self.load = load
    }

    /// The card's one control: a tap while the sound loads is dropped, so it never starts a second player.
    func toggle(_ file: SharedFile, entryId: String, model: BotStore) async {
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
        do {
            let player = try await prepared(file, entryId: entryId, model: model)
            guard generation == started else { state = .idle; return }
            // Heard with the ringer off too.
            let session = AVAudioSession.sharedInstance()
            if session.category != .playAndRecord {
                try session.setCategory(.playback)
                try session.setActive(true)
                ownsSession = true
            }
            guard player.play() else { throw CocoaError(.coderReadCorrupt) }
            elapsed = player.currentTime
            state = .playing
            startClock()
        } catch {
            // A card that went away mid-load never shows the failure.
            guard generation == started else { return }
            fail()
        }
    }

    func stop() {
        generation += 1
        player?.stop()
        player?.currentTime = 0
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

    /// Paused, stopped, ended or failed: other apps' audio may pick up again.
    private func releaseSession() {
        guard ownsSession else { return }
        ownsSession = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private func prepared(_ file: SharedFile, entryId: String, model: BotStore) async throws -> AVAudioPlayer {
        if let player { return player }
        guard let data = await load(model, file, entryId) else { throw CocoaError(.fileReadUnknown) }
        let player = try AVAudioPlayer(data: data)
        duration = player.duration
        let done = PlayDone { [weak self] in Task { @MainActor in self?.finish() } }
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
