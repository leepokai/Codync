import AVFoundation
import CodyncKit
import Foundation
import Observation
import os
@preconcurrency import WebRTC

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// OpenAI Realtime or GPT-Live over WebRTC: audio goes directly between this phone and OpenAI.
/// The host exchanges Live offers or mints Realtime credentials; Live event handling stays on the host.
@MainActor
@Observable
final class OpenAIRealtimeEngine: VoiceEngine {
    private(set) var phase: VoicePhase = .starting {
        didSet { onActivityChanged?(!ended && phase.holdsCall) }
    }
    private(set) var level: Float = 0
    var muted = false {
        didSet {
            mic?.isEnabled = !muted
            if muted { level = 0 }
        }
    }
    var provider: VoiceProvider? { .openAI }
    @ObservationIgnored var onActivityChanged: ((Bool) -> Void)?

    private let model: String
    private let voice: String
    /// The call on the computer: the client secret, the voice's instructions and tools, the transcript.
    private let link: CallLink
    private var session: CallSession?
    /// Tool calls still running on the computer: replies wait, so the model gets each result first.
    private var toolsRunning = 0
    private var pc: RTCPeerConnection?
    private var events: RTCDataChannel?
    private var mic: RTCAudioTrack?
    private var delegate: RealtimeEvents?
    private var ended = false
    /// A response is being generated; new replies wait for it to finish.
    private var responding = false
    private var pending: [String] = []
    private var dropTimer: Task<Void, Never>?
    /// The session was set up once: a drop after that is tried again instead of ending the call.
    private var connected = false
    private var reconnects = 0
    private var audioEvents: AudioSessionEvents?
    private var live: LiveCallTransport?
    private var livePending: [(CallCue.Kind, String)] = []
    private var activity: WebRTCAudioActivity?
    private var generation = 0
    private var outputSuppressed = false
    private var usesLive: Bool { LiveCallTransport.isModel(model) }

    init(model: String, voice: String, link: CallLink) {
        self.model = model
        self.voice = voice
        self.link = link
    }

    func start() async {
        guard await AVAudioApplication.requestRecordPermission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.", .microphone)
        }
        // WebRTC pauses and restarts its own audio; the call shows the pause.
        audioEvents = AudioSessionEvents(engine: nil, interrupted: { [weak self] in self?.interrupted($0) })
        await open()
    }

    /// A new provider session resumes the call using context retained on the host.
    private func open() async {
        let attempt = generation
        do {
            if usesLive { return try await connect(secret: nil) }
            let session = try await link.start("realtime", provider: .openAI, model: model, voice: voice)
            guard !ended, generation == attempt else { return }
            guard let secret = session.credential else { throw VoiceError("OpenAI sent no session credential.") }
            self.session = session
            try await connect(secret: secret)
        } catch {
            guard !ended, generation == attempt else { return }
            if connected { dropped(error.localizedDescription) } else { fail(error.localizedDescription, .provider) }
        }
    }

    private func connect(secret: String?) async throws {
        let audio = RTCAudioSessionConfiguration.webRTC()
        audio.categoryOptions = CallAudioSession.options
        RTCAudioSessionConfiguration.setWebRTC(audio)

        let config = RTCConfiguration()
        config.sdpSemantics = .unifiedPlan
        config.bundlePolicy = .maxBundle
        let attempt = generation
        let delegate = RealtimeEvents(owner: self, generation: attempt)
        guard let pc = Self.factory.peerConnection(with: config, constraints: Self.noConstraints, delegate: delegate) else {
            throw VoiceError( "Couldn't start the call audio.")
        }
        self.delegate = delegate
        self.pc = pc
        let source = Self.factory.audioSource(with: Self.noConstraints)
        let mic = Self.factory.audioTrack(with: source, trackId: "mic")
        mic.isEnabled = !muted
        pc.add(mic, streamIds: ["call"])
        self.mic = mic
        let events = pc.dataChannel(forLabel: "oai-events", configuration: RTCDataChannelConfiguration())
        events?.delegate = delegate
        self.events = events

        let offer = try await pc.offer(for: Self.noConstraints)
        guard !ended, generation == attempt else { throw CancellationError() }
        try await pc.setLocalDescription(offer)
        guard !ended, generation == attempt else { throw CancellationError() }
        let answer: String
        if usesLive {
            let transport = LiveCallTransport(link: link, send: { [weak self] data in
                guard let self, self.generation == attempt else { return }
                self.sendData(data)
            }, ready: { [weak self] in
                guard let self, self.generation == attempt else { return }
                self.becameReady()
            }, failed: { [weak self] message in
                guard let self, self.generation == attempt else { return }
                self.fail(message, .provider)
            })
            live = transport
            for (kind, text) in livePending { transport.update(kind, text) }
            livePending.removeAll()
            let deadline = ContinuousClock.now + .seconds(5)
            while pc.iceGatheringState != .complete && ContinuousClock.now < deadline {
                try await Task.sleep(for: .milliseconds(50))
                guard !ended, generation == attempt else { throw CancellationError() }
            }
            guard pc.iceGatheringState == .complete else {
                throw VoiceError("Could not prepare the GPT-Live audio connection. Try again.")
            }
            let result = try await link.client().startLiveCall(botId: link.botId, call: link.call, model: model,
                voice: voice, language: Locale.preferredLanguages.first, sdp: pc.localDescription?.sdp ?? offer.sdp)
            guard !ended, generation == attempt else { throw CancellationError() }
            transport.attach(result.sessionId)
            answer = result.sdp
        } else {
            guard let secret else { throw VoiceError("OpenAI sent no session credential.") }
            answer = try await realtimeAnswer(secret: secret, offer: offer.sdp)
        }
        guard !ended, generation == attempt else { throw CancellationError() }
        try await pc.setRemoteDescription(RTCSessionDescription(type: .answer, sdp: answer))
        guard !ended, generation == attempt else { throw CancellationError() }
        if usesLive {
            activity = WebRTCAudioActivity(peer: pc) { [weak self] speaking, level in
                guard let self, self.generation == attempt, !self.ended, self.phase.isActive else { return }
                self.level = self.muted ? 0 : level
                self.phase = speaking && !self.outputSuppressed ? .speaking : .listening
            }
        }
        dropTimer = Task { [weak self] in
            try? await Task.sleep(for: .seconds(15))
            guard let self, !Task.isCancelled, self.phase.isConnecting else { return }
            if self.connected { self.dropped("Couldn't reach OpenAI.") } else { self.fail("Couldn't reach OpenAI.", .connection) }
        }
    }

    func end() {
        guard !ended else { return }
        ended = true
        livePending.removeAll()
        onActivityChanged?(false)
        audioEvents?.stop()
        audioEvents = nil
        mic?.isEnabled = false
        pc?.receivers.forEach { $0.track?.isEnabled = false }
        activity?.stop()
        if let live {
            live.requestClose()
            Task { [weak self] in
                let deadline = ContinuousClock.now + .seconds(1)
                while !live.isClosed && ContinuousClock.now < deadline {
                    try? await Task.sleep(for: .milliseconds(50))
                }
                if !live.isClosed { log.info("GPT-Live closure was not confirmed before disconnecting.") }
                self?.close()
            }
        } else { close() }
    }

    private func close() {
        generation += 1
        dropTimer?.cancel()
        activity?.stop()
        activity = nil
        live?.stop()
        live = nil
        outputSuppressed = false
        events?.close()
        pc?.close()
        events = nil
        pc = nil
        mic = nil
        delegate = nil
        responding = false
        toolsRunning = 0
    }

    /// The connection to OpenAI dropped mid-call: a new session, primed by the computer with the call so far, a few
    /// times before the call falls back.
    private func dropped(_ reason: String) {
        guard !ended else { return }
        guard reconnects < Reconnect.delays.count else { return fail(reason, .connection) }
        let delay = Reconnect.delays[reconnects]
        reconnects += 1
        close()
        if phase != .interrupted { phase = .reconnecting }
        let attempt = generation
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !self.ended, self.generation == attempt else { return }
            await self.open()
        }
    }

    func resume() {
        guard !ended, phase == .interrupted else { return }
        phase = pc == nil ? .reconnecting : .listening
        flush()
    }

    private func interrupted(_ change: AudioInterruption) {
        guard !ended else { return }
        switch change {
        case .began:
            guard phase.isActive else { return }
            level = 0
            phase = .interrupted
        case .ended(let shouldResume):
            if shouldResume { resume() }
        }
    }

    func interrupt() {
        guard phase == .speaking else { return }
        if usesLive {
            outputSuppressed = true
            pc?.receivers.forEach { $0.track?.isEnabled = false }
            phase = .listening
            return
        }
        send(["type": "response.cancel"])
        send(["type": "output_audio_buffer.clear"])
        phase = .listening
    }

    func speak(reply: String) {
        guard !ended, !phase.isFailed else { return }
        if usesLive { return updateLive(.reply, reply) }
        pending.append(link.reply(reply))
        flush()
    }

    func announce(_ notice: String) {
        guard !ended, !phase.isFailed else { return }
        if usesLive { return updateLive(.notice, notice) }
        pending.append(link.notice(notice))
        flush()
    }

    /// Only the newest progress waits to be said.
    func progress(_ text: String) {
        guard !ended, !phase.isFailed else { return }
        if usesLive { return updateLive(.progress, text) }
        pending.removeAll { $0.hasPrefix(CallLink.progressTag) }
        pending.append(link.progress(text))
        flush()
    }

    private func updateLive(_ kind: CallCue.Kind, _ text: String) {
        if let live { return live.update(kind, text) }
        if kind == .progress { livePending.removeAll { $0.0 == .progress } }
        guard livePending.count < 32 else { return fail("Too many voice updates are waiting for GPT-Live.", .connection) }
        livePending.append((kind, text))
    }

    // MARK: events

    func opened(generation attempt: Int) {
        guard !ended, generation == attempt, !usesLive else { return }
        let declared = session.flatMap { (try? JSONSerialization.jsonObject(with: $0.tools)) as? [[String: Any]] } ?? []
        let tools: [[String: Any]] = declared.map {
            ["type": "function", "name": $0["name"] ?? "", "description": $0["description"] ?? "", "parameters": $0["parameters"] ?? [:]]
        }
        send([
            "type": "session.update",
            "session": [
                "type": "realtime",
                "instructions": session?.instructions ?? "",
                "tools": tools,
                // What the user says is transcribed too, for the call's line in the chat.
                "audio": ["input": [
                    "turn_detection": ["type": "semantic_vad"],
                    "transcription": ["model": VoiceSettings.transcribeModel(.openAI)],
                ]],
            ] as [String: Any],
        ])
    }

    func received(_ data: Data, generation attempt: Int? = nil) {
        guard attempt == nil || generation == attempt else { return }
        if let live {
            if !ended, outputSuppressed,
               let message = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
               message["type"] as? String == "session.input_transcript.delta" {
                outputSuppressed = false
                pc?.receivers.forEach { $0.track?.isEnabled = true }
            }
            live.receive(data)
            return
        }
        guard !ended, let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        switch msg["type"] as? String {
        // Connected once the session has the instructions and tools, not when the channel opens.
        case "session.updated" where phase.isConnecting:
            becameReady()
        case "input_audio_buffer.speech_started": level = muted ? 0 : 0.7
        case "input_audio_buffer.speech_stopped": level = 0
        case "input_audio_buffer.committed": link.transcribing(true)
        case "conversation.item.input_audio_transcription.failed": link.transcribing(false)
        case "output_audio_buffer.started" where phase.isActive: phase = .speaking
        case "output_audio_buffer.stopped", "output_audio_buffer.cleared": if phase == .speaking { phase = .listening }
        case "response.created": responding = true
        case "response.done":
            responding = false
            flush()
        case "conversation.item.input_audio_transcription.completed":
            link.add("user", msg["transcript"] as? String ?? "")
            link.endTurn()
        case "response.output_audio_transcript.done":
            link.add("voice", msg["transcript"] as? String ?? "")
            link.endTurn()
        case "response.function_call_arguments.done":
            guard let id = msg["call_id"] as? String, let name = msg["name"] as? String else { return }
            let args = Data((msg["arguments"] as? String ?? "{}").utf8)
            // The tool runs on the computer.
            toolsRunning += 1
            let attempt = generation
            Task { [weak self, link] in
                let output = String(decoding: await link.tool(name, arguments: args), as: UTF8.self)
                guard let self, self.generation == attempt else { return }
                self.toolsRunning -= 1
                guard !self.ended else { return }
                self.send(["type": "conversation.item.create", "item": ["type": "function_call_output", "call_id": id, "output": output]])
                // An empty entry asks for a response once this one is done, so the model speaks the result.
                self.pending.append("")
                self.flush()
            }
        case "error":
            let error = msg["error"] as? [String: Any]
            log.error("realtime: \(error?["message"] as? String ?? "?", privacy: .public)")
        default: break
        }
    }

    func iceChanged(_ state: RTCIceConnectionState, generation attempt: Int? = nil) {
        guard attempt == nil || generation == attempt else { return }
        // While connecting, only a failure counts: the start timer runs until the session is set up.
        guard !ended, !phase.isConnecting || state == .failed else { return }
        switch state {
        case .connected, .completed: if phase.isActive || phase == .interrupted { dropTimer?.cancel() }
        case .disconnected:
            dropTimer?.cancel()
            dropTimer = Task { [weak self] in
                try? await Task.sleep(for: .seconds(5))
                guard let self, !Task.isCancelled else { return }
                self.dropped("Lost the connection to OpenAI.")
            }
        case .failed: if connected { dropped("Lost the connection to OpenAI.") } else { fail("Couldn't reach OpenAI.", .connection) }
        default: break
        }
    }

    /// The events channel closed under a running session.
    func channelClosed(_ channel: ObjectIdentifier, generation attempt: Int) {
        guard !ended, generation == attempt, connected, events.map(ObjectIdentifier.init) == channel else { return }
        dropped("Lost the connection to OpenAI.")
    }

    /// Hands waiting replies (and tool results, as empty entries) to the model once it's free.
    private func flush() {
        guard !usesLive, !responding, toolsRunning == 0, phase.isActive, events?.readyState == .open, !pending.isEmpty else { return }
        for text in pending where !text.isEmpty {
            send(["type": "conversation.item.create", "item": [
                "type": "message", "role": "user", "content": [["type": "input_text", "text": text]],
            ] as [String: Any]])
        }
        pending.removeAll()
        responding = true
        send(["type": "response.create"])
    }

    private func becameReady() {
        guard !ended else { return }
        dropTimer?.cancel()
        connected = true
        reconnects = 0
        phase = .listening
        flush()
    }

    private func sendData(_ data: Data) {
        guard let events, events.readyState == .open else { return }
        events.sendData(RTCDataBuffer(data: data, isBinary: false))
    }

    private func send(_ event: [String: Any]) {
        guard let events, events.readyState == .open,
              let data = try? JSONSerialization.data(withJSONObject: event) else { return }
        events.sendData(RTCDataBuffer(data: data, isBinary: false))
    }

    private func fail(_ message: String, _ kind: VoiceFailureKind) {
        end()
        phase = .failed(message, kind)
    }

    private func realtimeAnswer(secret: String, offer: String) async throws -> String {
        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/realtime/calls")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(secret)", forHTTPHeaderField: "Authorization")
        request.setValue("application/sdp", forHTTPHeaderField: "Content-Type")
        request.httpBody = Data(offer.utf8)
        request.timeoutInterval = 15
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            let message = (json?["error"] as? [String: Any])?["message"] as? String
            throw VoiceError( message ?? "OpenAI answered \(status).")
        }
        return String(decoding: data, as: UTF8.self)
    }

    private static let factory: RTCPeerConnectionFactory = {
        RTCInitializeSSL()
        return RTCPeerConnectionFactory()
    }()

    private static let noConstraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
}
