import AVFoundation
import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// Gemini Live over a WebSocket, phone ↔ Google directly: 16 kHz PCM up, 24 kHz PCM down,
/// echo-cancelled by the voice-processing input so you can talk over it. The user's key stays on the
/// computer, which mints one-use ephemeral tokens; a session the server ends (`goAway`) is resumed with a fresh one.
@MainActor
@Observable
final class GeminiLiveEngine: VoiceEngine {
    private(set) var phase: VoicePhase = .starting {
        didSet { onActivityChanged?(!ended && phase.isActive) }
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

    /// A short-lived credential for the provider, minted by the computer that holds the key.
    private let credential: @MainActor () async throws -> String
    private let model: String
    private let voice: String
    private let op: CallOperator
    private var socket: URLSessionWebSocketTask?
    /// Setup is done on the current socket.
    private var ready = false
    private var resumeHandle: String?
    private var reconnects = 0
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private var audioRunning = false
    /// Buffers scheduled and not played yet; bumping the generation drops late callbacks.
    private var queued = 0
    private var playGeneration = 0
    /// The model finished its turn (nothing more is coming until we or the user speak).
    private var turnDone = true
    /// After a local interrupt, the rest of the turn's audio is dropped.
    private var dropping = false
    private var pending: [String] = []
    private var ended = false

    private static let playFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 24000, channels: 1, interleaved: false)

    init(model: String, voice: String, credential: @escaping @MainActor () async throws -> String, op: CallOperator) {
        self.credential = credential
        self.model = model
        self.voice = voice
        self.op = op
    }

    func start() async {
        guard await AVAudioApplication.requestRecordPermission() else {
            return fail("Allow the microphone for Codync in Settings to talk to your bots.")
        }
        await connect()
    }

    private func connect() async {
        do {
            let token = try await credential()
            guard !ended else { return }
            var url = URLComponents(string: "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContentConstrained")
            url?.queryItems = [URLQueryItem(name: "access_token", value: token)]
            guard let url = url?.url else { throw VoiceError( "Couldn't reach Gemini.") }
            let socket = URLSession.shared.webSocketTask(with: url)
            socket.maximumMessageSize = 16 << 20
            self.socket = socket
            ready = false
            socket.resume()
            receive(socket)
            sendSetup(on: socket)
        } catch {
            guard !ended else { return }
            fail(error.localizedDescription)
        }
    }

    private func sendSetup(on socket: URLSessionWebSocketTask) {
        let tools: [[String: Any]] = CallOperator.tools.map { tool in
            var declaration: [String: Any] = ["name": tool.name, "description": tool.description]
            if let properties = tool.parameters["properties"] as? [String: any Sendable], !properties.isEmpty {
                declaration["parameters"] = Self.geminiSchema(tool.parameters)
            }
            return declaration
        }
        let setup: [String: Any] = [
            "model": "models/\(model)",
            "generationConfig": [
                "responseModalities": ["AUDIO"],
                "speechConfig": ["voiceConfig": ["prebuiltVoiceConfig": ["voiceName": voice]]],
            ] as [String: Any],
            "systemInstruction": ["parts": [["text": op.instructions]]],
            "tools": [["functionDeclarations": tools]],
            // Audio sessions are otherwise capped at 15 minutes.
            "contextWindowCompression": ["slidingWindow": [String: Any]()],
            "sessionResumption": resumeHandle.map { ["handle": $0] } ?? [:],
        ]
        send(["setup": setup], on: socket)
    }

    func end() {
        ended = true
        onActivityChanged?(false)
        socket?.cancel(with: .normalClosure, reason: nil)
        socket = nil
        stopPlayback()
        if audioRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            audioRunning = false
        }
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func interrupt() {
        guard phase == .speaking else { return }
        stopPlayback()
        dropping = !turnDone
        phase = .listening
    }

    func speak(reply: String) {
        guard !ended, !phase.isFailed else { return }
        pending.append(op.reply(reply))
        flush()
    }

    func announce(_ notice: String) {
        guard !ended, !phase.isFailed else { return }
        pending.append(op.notice(notice))
        flush()
    }

    // MARK: socket

    private func receive(_ socket: URLSessionWebSocketTask) {
        Task { [weak self] in
            while true {
                let message: URLSessionWebSocketTask.Message
                do {
                    message = try await socket.receive()
                } catch {
                    self?.closed(socket)
                    return
                }
                guard let self, self.socket === socket else { return }
                switch message {
                case .data(let data): self.received(data)
                case .string(let text): self.received(Data(text.utf8))
                @unknown default: break
                }
            }
        }
    }

    private func closed(_ socket: URLSessionWebSocketTask) {
        guard !ended, self.socket === socket else { return }
        self.socket = nil
        ready = false
        let reason = socket.closeReason.map { String(decoding: $0, as: UTF8.self) } ?? ""
        log.info("gemini closed: \(reason, privacy: .public)")
        if resumeHandle != nil, reconnects < 3 {
            reconnects += 1
            Task { await connect() }
        } else {
            fail(reason.isEmpty ? "Lost the connection to Gemini." : reason)
        }
    }

    private func received(_ data: Data) {
        guard !ended, let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        if msg["setupComplete"] != nil {
            ready = true
            reconnects = 0
            if !audioRunning {
                do {
                    try startAudio()
                } catch {
                    return fail("Couldn't use the microphone: \(error.localizedDescription)")
                }
            }
            if phase == .starting { phase = .listening }
            flush()
        }
        if let content = msg["serverContent"] as? [String: Any] {
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
            if content["turnComplete"] as? Bool == true {
                turnDone = true
                dropping = false
                if queued == 0, phase == .speaking { phase = .listening }
                flush()
            }
        }
        if let calls = (msg["toolCall"] as? [String: Any])?["functionCalls"] as? [[String: Any]] {
            let responses: [[String: Any]] = calls.compactMap { call in
                guard let id = call["id"] as? String, let name = call["name"] as? String else { return nil }
                let output = op.call(name, arguments: call["args"] as? [String: Any] ?? [:])
                let result = (try? JSONSerialization.jsonObject(with: Data(output.utf8))) ?? [:]
                return ["id": id, "name": name, "response": ["result": result]]
            }
            send(["toolResponse": ["functionResponses": responses]])
        }
        if let update = msg["sessionResumptionUpdate"] as? [String: Any],
           update["resumable"] as? Bool == true, let handle = update["newHandle"] as? String {
            resumeHandle = handle
        }
        if msg["goAway"] != nil {
            // The server ends this connection soon; carry on in a resumed one.
            socket?.cancel(with: .normalClosure, reason: nil)
        }
    }

    private func send(_ message: [String: Any], on socket: URLSessionWebSocketTask? = nil) {
        guard let socket = socket ?? (ready ? self.socket : nil),
              let data = try? JSONSerialization.data(withJSONObject: message) else { return }
        socket.send(.string(String(decoding: data, as: UTF8.self))) { error in
            if let error { log.error("gemini send: \(error.localizedDescription, privacy: .public)") }
        }
    }

    /// Replies wait for the model to finish talking; `turnComplete` here would cut it off.
    private func flush() {
        guard ready, turnDone, queued == 0, !pending.isEmpty else { return }
        let text = pending.joined(separator: "\n\n")
        pending.removeAll()
        turnDone = false
        send(["clientContent": ["turns": [["role": "user", "parts": [["text": text]]]], "turnComplete": true] as [String: Any]])
    }

    // MARK: audio

    private func startAudio() throws {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .voiceChat, options: [.defaultToSpeaker, .allowBluetoothHFP])
            try session.setActive(true)
        let input = engine.inputNode
        try input.setVoiceProcessingEnabled(true)
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: Self.playFormat)
        let format = try MicTap.format(of: input)
        input.installTap(onBus: 0, bufferSize: 2048, format: format, block: MicTap.pcm16k(from: format) { [weak self] pcm, level in
            let audio = pcm.base64EncodedString()
            Task { @MainActor in self?.captured(audio, level: level) }
        })
        engine.prepare()
        try engine.start()
        player.play()
        audioRunning = true
    }

    private func captured(_ audio: String, level: Float) {
        guard !ended, !muted, phase.isActive else { return }
        self.level = phase == .speaking ? 0 : level
        send(["realtimeInput": ["audio": ["data": audio, "mimeType": "audio/pcm;rate=16000"]]])
    }

    private func play(_ base64: String) {
        guard let format = Self.playFormat, let data = Data(base64Encoded: base64), data.count >= 2 else { return }
        let frames = data.count / 2
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames)),
              let out = buffer.floatChannelData?[0] else { return }
        buffer.frameLength = AVAudioFrameCount(frames)
        data.withUnsafeBytes { raw in
            for i in 0..<frames {
                let sample = Int16(littleEndian: raw.loadUnaligned(fromByteOffset: i * 2, as: Int16.self))
                out[i] = Float(sample) / 32768
            }
        }
        queued += 1
        level = 0
        phase = .speaking
        let generation = playGeneration
        player.scheduleBuffer(buffer, completionCallbackType: .dataPlayedBack) { [weak self] _ in
            Task { @MainActor in self?.played(generation) }
        }
    }

    private func played(_ generation: Int) {
        guard generation == playGeneration, !ended else { return }
        queued = max(0, queued - 1)
        guard queued == 0, turnDone else { return }
        if phase == .speaking { phase = .listening }
        flush()
    }

    private func stopPlayback() {
        playGeneration += 1
        queued = 0
        if audioRunning {
            player.stop()
            player.play()
        }
    }

    private func fail(_ message: String) {
        end()
        phase = .failed(message)
    }

    // MARK: helpers

    /// Gemini's schema spells types in capitals (`OBJECT`, `STRING`).
    private static func geminiSchema(_ schema: [String: any Sendable]) -> [String: Any] {
        var out: [String: Any] = [:]
        for (key, value) in schema {
            switch value {
            case let type as String where key == "type": out[key] = type.uppercased()
            case let nested as [String: any Sendable] where key == "properties":
                out[key] = nested.mapValues { ($0 as? [String: any Sendable]).map(geminiSchema) ?? [:] }
            default: out[key] = value
            }
        }
        return out
    }
}
