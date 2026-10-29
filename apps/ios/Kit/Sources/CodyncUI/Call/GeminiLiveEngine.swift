import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// Gemini Live over a WebSocket, phone ↔ Google directly: 16 kHz PCM up, 24 kHz PCM down,
/// echo-cancelled by the voice-processing input so you can talk over it. The computer runs the call (`CallLink`): it
/// mints one-use ephemeral tokens on the user's key and writes the voice's instructions and tools, again for each
/// session; a session the server ends (`goAway`) is resumed with a fresh one, and a dropped one is tried again.
@MainActor
@Observable
final class GeminiLiveEngine: VoiceEngine {
    private(set) var phase: VoicePhase = .starting {
        didSet { onActivityChanged?(!ended && phase.holdsCall) }
    }
    private(set) var level: Float = 0
    var muted = false {
        didSet {
            guard muted != oldValue else { return }
            level = 0
            if muted { send(["realtimeInput": ["audioStreamEnd": true]]) }
        }
    }
    var provider: VoiceProvider? { .gemini }
    @ObservationIgnored var onActivityChanged: ((Bool) -> Void)?
    /// Playback stopped reporting progress after the voice's turn, and the call went back to listening.
    @ObservationIgnored var onStall: (() -> Void)?

    private let model: String
    private let voice: String
    private let link: CallLink
    private let audio: any CallAudio
    private let openSocket: @MainActor (URL) -> any LiveSocket
    /// The current provider session's instructions and tools, from the computer.
    private var session: CallSession?
    /// Tool calls still running on the computer: replies wait, so the model gets each answer first.
    private var toolsRunning = 0
    private var socket: (any LiveSocket)?
    /// Setup is done on the current socket.
    private var ready = false
    /// A session was set up once: a drop after that is tried again instead of ending the call.
    private var connected = false
    /// The server said it ends this connection (`goAway`): carry on in a resumed one at once.
    private var replacing = false
    private var resumeHandle: String?
    private var reconnects = 0
    private var audioStarted = false
    /// Pieces handed to the speaker and not rendered yet; bumping the generation drops late callbacks.
    private var queued = 0
    private var playGeneration = 0
    private var lastPlayed = ContinuousClock.now
    private var watchdog: Task<Void, Never>?
    /// The model finished its turn (nothing more is coming until we or the user speak).
    private var turnDone = true
    /// After a local interrupt, the rest of the turn's audio is dropped.
    private var dropping = false
    private var pending: [String] = []
    private var ended = false

    /// Without playback progress this long after the voice's turn, the call stops waiting for the speaker (#214:
    /// buffers that never finish would keep it on "speaking" and the bot's replies unsaid).
    static var stallTimeout: Duration = .seconds(4)

    init(model: String, voice: String, link: CallLink, audio: any CallAudio = VoiceProcessingAudio(),
         socket: @escaping @MainActor (URL) -> any LiveSocket = { URLSessionLiveSocket($0) }) {
        self.model = model
        self.voice = voice
        self.link = link
        self.audio = audio
        openSocket = socket
    }

    func start() async {
        guard await audio.permission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.", .microphone)
        }
        audio.interruption = { [weak self] in self?.interrupted($0) }
        await connect()
    }

    private func connect() async {
        do {
            let session = try await link.start("realtime", provider: .gemini, model: model, voice: voice)
            guard !ended else { return }
            guard let token = session.credential else { throw VoiceError("Gemini sent no session credential.") }
            self.session = session
            var url = URLComponents(string: "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContentConstrained")
            url?.queryItems = [URLQueryItem(name: "access_token", value: token)]
            guard let url = url?.url else { throw VoiceError("Couldn't reach Gemini.") }
            let socket = openSocket(url)
            self.socket = socket
            ready = false
            receive(socket)
            sendSetup(on: socket)
        } catch {
            guard !ended else { return }
            // Starting fails over; mid-call, the computer or Google may be back in a moment.
            if connected { dropped(error.localizedDescription) } else { fail(error.localizedDescription, .provider) }
        }
    }

    private func sendSetup(on socket: any LiveSocket) {
        let declared = session.flatMap { (try? JSONSerialization.jsonObject(with: $0.tools)) as? [[String: Any]] } ?? []
        let tools: [[String: Any]] = declared.map { tool in
            var declaration: [String: Any] = ["name": tool["name"] ?? "", "description": tool["description"] ?? ""]
            if let parameters = tool["parameters"] as? [String: Any], let properties = parameters["properties"] as? [String: Any],
               !properties.isEmpty {
                declaration["parameters"] = Self.geminiSchema(parameters)
            }
            return declaration
        }
        let setup: [String: Any] = [
            "model": "models/\(model)",
            "generationConfig": [
                "responseModalities": ["AUDIO"],
                "speechConfig": ["voiceConfig": ["prebuiltVoiceConfig": ["voiceName": voice]]],
            ] as [String: Any],
            "systemInstruction": ["parts": [["text": session?.instructions ?? ""]]],
            "tools": [["functionDeclarations": tools]],
            // Audio sessions are otherwise capped at 15 minutes.
            "contextWindowCompression": ["slidingWindow": [String: Any]()],
            // What both sides said, for the call's line in the chat.
            "inputAudioTranscription": [String: Any](),
            "outputAudioTranscription": [String: Any](),
            "sessionResumption": resumeHandle.map { ["handle": $0] } ?? [:],
        ]
        send(["setup": setup], on: socket)
    }

    func end() {
        ended = true
        onActivityChanged?(false)
        socket?.close()
        socket = nil
        stopPlayback()
        audio.interruption = nil
        if audioStarted {
            audio.stop()
            audioStarted = false
        }
    }

    func interrupt() {
        guard phase == .speaking else { return }
        stopPlayback()
        dropping = !turnDone
        phase = .listening
    }

    func speak(reply: String) {
        guard !ended, !phase.isFailed else { return }
        pending.append(link.reply(reply))
        flush()
    }

    func announce(_ notice: String) {
        guard !ended, !phase.isFailed else { return }
        pending.append(link.notice(notice))
        flush()
    }

    /// Only the newest progress waits to be said.
    func progress(_ text: String) {
        guard !ended, !phase.isFailed else { return }
        pending.removeAll { $0.hasPrefix(CallLink.progressTag) }
        pending.append(link.progress(text))
        flush()
    }

    /// The audio is back (the user tapped the call's line, or the interruption ended): listening once the
    /// microphone delivers again.
    func resume() {
        guard !ended, phase == .interrupted else { return }
        do {
            try audio.resume()
        } catch {
            return fail("Couldn't use the microphone: \(error.localizedDescription)", .microphone)
        }
        phase = .reconnecting
    }

    private func interrupted(_ change: AudioInterruption) {
        guard !ended else { return }
        switch change {
        case .began:
            guard phase.holdsCall || phase == .starting else { return }
            stopPlayback()
            dropping = !turnDone
            level = 0
            phase = .interrupted
            send(["realtimeInput": ["audioStreamEnd": true]])
        case .ended(let shouldResume):
            if shouldResume { resume() }
        }
    }

    // MARK: socket

    private func receive(_ socket: any LiveSocket) {
        Task { [weak self] in
            while true {
                let text: String
                do {
                    text = try await socket.receive()
                } catch {
                    self?.closed(socket)
                    return
                }
                guard let self, self.socket === socket else { return }
                self.received(Data(text.utf8))
            }
        }
    }

    private func closed(_ socket: any LiveSocket) {
        guard !ended, self.socket === socket else { return }
        self.socket = nil
        ready = false
        let (code, reason) = (socket.closeCode, socket.closeReason)
        log.info("gemini closed: \(code) \(reason, privacy: .public)")
        guard connected else { return fail(reason.isEmpty ? "Couldn't reach Gemini." : reason, .provider) }
        // Gemini refused the session (a key, a model, a request): trying again won't help.
        if code == 1007 || code == 1008 { return fail(reason.isEmpty ? "Gemini ended the call." : reason, .provider) }
        if replacing {
            replacing = false
            // The turn under way ends with this connection: the resumed one won't complete it.
            turnEnded()
            Task { await connect() }
        } else {
            dropped(reason)
        }
    }

    /// The connection to Gemini dropped mid-call: tried again a few times (a resumed session when Gemini gave a handle,
    /// else a fresh one the computer primes with the call so far) before the call falls back.
    private func dropped(_ reason: String) {
        guard reconnects < Reconnect.delays.count else {
            return fail(reason.isEmpty ? "Lost the connection to Gemini." : reason, .connection)
        }
        let delay = Reconnect.delays[reconnects]
        reconnects += 1
        stopPlayback()
        turnDone = true
        dropping = false
        if phase != .interrupted { phase = .reconnecting }
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !self.ended else { return }
            await self.connect()
        }
    }

    private func received(_ data: Data) {
        guard !ended, let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        if msg["setupComplete"] != nil { setupCompleted() }
        if let content = msg["serverContent"] as? [String: Any] {
            if let heard = (content["inputTranscription"] as? [String: Any])?["text"] as? String { link.add("user", heard) }
            if let said = (content["outputTranscription"] as? [String: Any])?["text"] as? String { link.add("voice", said) }
            if content["turnComplete"] as? Bool == true || content["interrupted"] as? Bool == true { link.endTurn() }
            if content["interrupted"] as? Bool == true {
                stopPlayback()
                dropping = false
                if phase == .speaking { phase = .listening }
            }
            if let parts = (content["modelTurn"] as? [String: Any])?["parts"] as? [[String: Any]] {
                turnDone = false
                for part in parts {
                    if let audio = (part["inlineData"] as? [String: Any])?["data"] as? String, !dropping { play(audio) }
                }
            }
            if content["turnComplete"] as? Bool == true { turnEnded() }
        }
        if let calls = (msg["toolCall"] as? [String: Any])?["functionCalls"] as? [[String: Any]] {
            let asked: [(id: String, name: String, args: Data)] = calls.compactMap { call in
                guard let id = call["id"] as? String, let name = call["name"] as? String else { return nil }
                let args = (try? JSONSerialization.data(withJSONObject: call["args"] as? [String: Any] ?? [:])) ?? Data("{}".utf8)
                return (id, name, args)
            }
            // The tools run on the computer; their answers go back together.
            toolsRunning += 1
            Task { [weak self, link] in
                var responses: [[String: Any]] = []
                for call in asked {
                    let result = (try? JSONSerialization.jsonObject(with: await link.tool(call.name, arguments: call.args))) ?? [:]
                    responses.append(["id": call.id, "name": call.name, "response": ["result": result]])
                }
                guard let self else { return }
                self.toolsRunning -= 1
                self.send(["toolResponse": ["functionResponses": responses]])
                self.flush()
            }
        }
        if let update = msg["sessionResumptionUpdate"] as? [String: Any],
           update["resumable"] as? Bool == true, let handle = update["newHandle"] as? String {
            resumeHandle = handle
        }
        if msg["goAway"] != nil {
            // The server ends this connection soon; carry on in a resumed one.
            replacing = true
            socket?.close()
        }
    }

    private func setupCompleted() {
        ready = true
        connected = true
        reconnects = 0
        if !audioStarted {
            do {
                try audio.start { [weak self] audio, level in self?.captured(audio, level: level) }
                audioStarted = true
            } catch {
                return fail("Couldn't use the microphone: \(error.localizedDescription)", .microphone)
            }
        }
        flush()
    }

    /// The model's turn is over: listening once what's queued has played.
    private func turnEnded() {
        turnDone = true
        dropping = false
        if queued == 0 { finishSpeaking() } else { watch() }
    }

    private func send(_ message: [String: Any], on socket: (any LiveSocket)? = nil) {
        guard let socket = socket ?? (ready ? self.socket : nil),
              let data = try? JSONSerialization.data(withJSONObject: message) else { return }
        socket.send(String(decoding: data, as: UTF8.self))
    }

    /// Replies wait for the model to finish talking (`turnComplete` here would cut it off), and for the audio to be
    /// back after an interruption.
    private func flush() {
        guard ready, turnDone, queued == 0, toolsRunning == 0, phase != .interrupted, !pending.isEmpty else { return }
        let text = pending.joined(separator: "\n\n")
        pending.removeAll()
        turnDone = false
        send(["clientContent": ["turns": [["role": "user", "parts": [["text": text]]]], "turnComplete": true] as [String: Any]])
    }

    // MARK: audio

    private func captured(_ audio: String, level: Float) {
        // Ready once the microphone delivers after setup (again, after a reconnect), not when the socket opens.
        if !ended, ready, phase.isConnecting {
            phase = .listening
            flush()
        }
        guard !ended, !muted, phase.isActive else { return }
        self.level = phase == .speaking ? 0 : level
        send(["realtimeInput": ["audio": ["data": audio, "mimeType": "audio/pcm;rate=16000"]]])
    }

    private func play(_ base64: String) {
        // Another app has the audio: what the model says meanwhile isn't heard.
        guard phase != .interrupted, let data = Data(base64Encoded: base64), data.count >= 2 else { return }
        queued += 1
        level = 0
        phase = .speaking
        let generation = playGeneration
        audio.play(data) { [weak self] in self?.played(generation) }
    }

    private func played(_ generation: Int) {
        guard generation == playGeneration, !ended else { return }
        lastPlayed = .now
        queued = max(0, queued - 1)
        guard queued == 0, turnDone else { return }
        finishSpeaking()
    }

    private func finishSpeaking() {
        watchdog?.cancel()
        watchdog = nil
        if phase == .speaking { phase = .listening }
        flush()
    }

    /// After the voice's turn, playback that stops reporting progress doesn't hold the call (see [`stallTimeout`]).
    private func watch() {
        watchdog?.cancel()
        lastPlayed = .now
        watchdog = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: Self.stallTimeout)
                guard let self, !Task.isCancelled, !self.ended, self.queued > 0, self.turnDone else { return }
                guard ContinuousClock.now - self.lastPlayed >= Self.stallTimeout else { continue }
                log.error("gemini playback stalled; back to listening")
                self.playGeneration += 1
                self.queued = 0
                self.onStall?()
                self.finishSpeaking()
                return
            }
        }
    }

    private func stopPlayback() {
        watchdog?.cancel()
        watchdog = nil
        playGeneration += 1
        queued = 0
        if audioStarted { audio.stopPlayback() }
    }

    private func fail(_ message: String, _ kind: VoiceFailureKind) {
        end()
        phase = .failed(message, kind)
    }

    // MARK: helpers

    /// Gemini's schema spells types in capitals (`OBJECT`, `STRING`).
    private static func geminiSchema(_ schema: [String: Any]) -> [String: Any] {
        var out: [String: Any] = [:]
        for (key, value) in schema {
            switch value {
            case let type as String where key == "type": out[key] = type.uppercased()
            case let nested as [String: Any] where key == "properties":
                out[key] = nested.mapValues { ($0 as? [String: Any]).map(geminiSchema) ?? [:] }
            default: out[key] = value
            }
        }
        return out
    }
}
