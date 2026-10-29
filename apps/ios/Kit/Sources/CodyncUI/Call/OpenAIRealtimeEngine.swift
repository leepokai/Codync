import AVFoundation
import CodyncKit
import Foundation
import Observation
import os
@preconcurrency import WebRTC

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// OpenAI Realtime over WebRTC, phone ↔ OpenAI directly: the microphone is a WebRTC audio track,
/// the model's voice plays from the remote track, and events (tools, replies) go over `oai-events`.
/// The user's key stays on the computer, which mints the client secret; the host never sees audio.
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

    /// A session with OpenAI: the computer mints its client secret and writes the voice's instructions (with the call
    /// so far, for a session that replaces a dropped one).
    private func open() async {
        do {
            let session = try await link.start("realtime", provider: .openAI, model: model, voice: voice)
            guard !ended else { return }
            guard let secret = session.credential else { throw VoiceError("OpenAI sent no session credential.") }
            self.session = session
            try await connect(secret: secret)
        } catch {
            guard !ended else { return }
            if connected { dropped(error.localizedDescription) } else { fail(error.localizedDescription, .provider) }
        }
    }

    private func connect(secret: String) async throws {
        let audio = RTCAudioSessionConfiguration.webRTC()
        audio.categoryOptions = CallAudioSession.options
        RTCAudioSessionConfiguration.setWebRTC(audio)

        let config = RTCConfiguration()
        config.sdpSemantics = .unifiedPlan
        config.bundlePolicy = .maxBundle
        let delegate = RealtimeEvents(owner: self)
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
        try await pc.setLocalDescription(offer)
        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/realtime/calls")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(secret)", forHTTPHeaderField: "Authorization")
        request.setValue("application/sdp", forHTTPHeaderField: "Content-Type")
        request.httpBody = Data(offer.sdp.utf8)
        request.timeoutInterval = 15
        let (data, response) = try await URLSession.shared.data(for: request)
        guard !ended else { return }
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            let message = (json?["error"] as? [String: Any])?["message"] as? String
            throw VoiceError( message ?? "OpenAI answered \(status).")
        }
        try await pc.setRemoteDescription(RTCSessionDescription(type: .answer, sdp: String(decoding: data, as: UTF8.self)))
        dropTimer = Task { [weak self] in
            try? await Task.sleep(for: .seconds(15))
            guard let self, !Task.isCancelled, self.phase.isConnecting else { return }
            if self.connected { self.dropped("Couldn't reach OpenAI.") } else { self.fail("Couldn't reach OpenAI.", .connection) }
        }
    }

    func end() {
        ended = true
        onActivityChanged?(false)
        audioEvents?.stop()
        audioEvents = nil
        close()
    }

    private func close() {
        dropTimer?.cancel()
        events?.close()
        pc?.close()
        events = nil
        pc = nil
        mic = nil
        delegate = nil
        responding = false
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
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !self.ended else { return }
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
        send(["type": "response.cancel"])
        send(["type": "output_audio_buffer.clear"])
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

    // MARK: events

    fileprivate func opened() {
        guard !ended else { return }
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

    func received(_ data: Data) {
        guard !ended, let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        switch msg["type"] as? String {
        // Connected once the session has the instructions and tools, not when the channel opens.
        case "session.updated" where phase.isConnecting:
            dropTimer?.cancel()
            connected = true
            reconnects = 0
            phase = .listening
            flush()
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
            Task { [weak self, link] in
                let output = String(decoding: await link.tool(name, arguments: args), as: UTF8.self)
                guard let self else { return }
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

    func iceChanged(_ state: RTCIceConnectionState) {
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
    fileprivate func channelClosed(_ channel: ObjectIdentifier) {
        guard !ended, connected, events.map(ObjectIdentifier.init) == channel else { return }
        dropped("Lost the connection to OpenAI.")
    }

    /// Hands waiting replies (and tool results, as empty entries) to the model once it's free.
    private func flush() {
        guard !responding, toolsRunning == 0, phase.isActive, events?.readyState == .open, !pending.isEmpty else { return }
        for text in pending where !text.isEmpty {
            send(["type": "conversation.item.create", "item": [
                "type": "message", "role": "user", "content": [["type": "input_text", "text": text]],
            ] as [String: Any]])
        }
        pending.removeAll()
        responding = true
        send(["type": "response.create"])
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

    private static let factory: RTCPeerConnectionFactory = {
        RTCInitializeSSL()
        return RTCPeerConnectionFactory()
    }()

    private static let noConstraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
}

/// WebRTC callbacks arrive on its signaling thread; this hops them to the main actor.
private final class RealtimeEvents: NSObject, RTCPeerConnectionDelegate, RTCDataChannelDelegate, Sendable {
    private let owner: WeakEngine

    @MainActor init(owner: OpenAIRealtimeEngine) { self.owner = WeakEngine(owner) }

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        let owner = owner
        Task { @MainActor in owner.value?.iceChanged(newState) }
    }

    func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {
        let (state, channel) = (dataChannel.readyState, ObjectIdentifier(dataChannel))
        let owner = owner
        Task { @MainActor in
            if state == .open { owner.value?.opened() } else if state == .closed { owner.value?.channelClosed(channel) }
        }
    }

    func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        let data = buffer.data
        let owner = owner
        Task { @MainActor in owner.value?.received(data) }
    }
}

private final class WeakEngine: Sendable {
    @MainActor weak var value: OpenAIRealtimeEngine?
    @MainActor init(_ value: OpenAIRealtimeEngine) { self.value = value }
}
