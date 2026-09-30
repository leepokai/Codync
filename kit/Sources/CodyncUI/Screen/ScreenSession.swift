#if os(iOS)
import CodyncKit
import Foundation
import Observation
import os
@preconcurrency import WebRTC

private let log = Logger(subsystem: "com.pokai.Codync", category: "Screen")

/// One live view of the computer's screen: a WebRTC peer that receives the
/// display as H.264 and sends input over two data channels (`input` reliable,
/// `input-fast` unordered for pointer moves). Signaling goes through the host
/// API, non-trickle (offer and answer carry every candidate).
@MainActor
@Observable
public final class ScreenSession {
    public enum Phase: Equatable {
        case connecting
        case live
        case reconnecting
        case failed(String)
        case closed
    }

    public private(set) var phase: Phase = .connecting
    public private(set) var track: RTCVideoTrack?
    /// Text the computer copied, waiting for the user to pull it onto the phone.
    public private(set) var remoteClipboard: String?
    public private(set) var lastError: String?
    public private(set) var display: ScreenDisplay?
    /// The display region the video currently shows (points; `nil` = all of it), as confirmed by the computer.
    public private(set) var streamCrop: CGRect?

    private let client: HostClient
    private var session: String?
    private var pc: RTCPeerConnection?
    private var reliable: RTCDataChannel?
    private var fast: RTCDataChannel?
    private var events: PeerEvents?
    private var gathered: CheckedContinuation<Void, Never>?
    private var recoverTask: Task<Void, Never>?
    private var gatherTask: Task<Void, Never>?
    private var renewTask: Task<Void, Never>?
    private var connectionTimer: Task<Void, Never>?
    private var generation = 0

    static let factory: RTCPeerConnectionFactory = {
        RTCInitializeSSL()
        return RTCPeerConnectionFactory(encoderFactory: RTCDefaultVideoEncoderFactory(), decoderFactory: RTCDefaultVideoDecoderFactory())
    }()

    public init(client: HostClient, display: ScreenDisplay?) {
        self.client = client
        self.display = display
    }

    // MARK: connection

    public func start() async {
        phase = .connecting
        do {
            try await connect()
        } catch {
            guard phase != .closed else { return }
            close()
            phase = .failed(error.localizedDescription)
        }
    }

    private func connect() async throws {
        let preparing = generation
        let connection = try await client.screenPrepare()
        guard generation == preparing, phase != .closed, !Task.isCancelled else {
            try? await client.screenClose(session: connection.session)
            throw CancellationError()
        }
        // Reserve the replacement before closing the old viewer so takeover survives renewal.
        let old = session
        teardown()
        let current = generation
        session = connection.session
        if let old { try? await client.screenClose(session: old) }
        try Task.checkCancellation()
        guard generation == current, phase != .closed else { throw CancellationError() }
        let config = RTCConfiguration()
        config.sdpSemantics = .unifiedPlan
        config.iceServers = connection.iceServers.map {
            RTCIceServer(urlStrings: $0.urls, username: $0.username, credential: $0.credential)
        }
        config.tcpCandidatePolicy = .enabled
        config.continualGatheringPolicy = .gatherOnce
        config.bundlePolicy = .maxBundle
        let events = PeerEvents(owner: self, generation: current)
        guard let pc = Self.factory.peerConnection(with: config, constraints: Self.noConstraints, delegate: events) else {
            throw HostError.http(0, "Couldn't start the video connection.")
        }
        self.events = events
        self.pc = pc

        let initVideo = RTCRtpTransceiverInit()
        initVideo.direction = .recvOnly
        guard let video = pc.addTransceiver(of: .video, init: initVideo) else {
            throw HostError.http(0, "Couldn't start the video connection.")
        }
        // Hardware H.264 first (High, then Constrained Baseline); the rest only as a fallback.
        let codecs = Self.factory.rtpReceiverCapabilities(forKind: kRTCMediaStreamTrackKindVideo).codecs
        let h264 = codecs.filter { $0.name == kRTCVideoCodecH264Name }
            .sorted { ($0.parameters["profile-level-id"] ?? "").hasPrefix("640c") && !($1.parameters["profile-level-id"] ?? "").hasPrefix("640c") }
        try? video.setCodecPreferences(h264 + codecs.filter { $0.name != kRTCVideoCodecH264Name }, error: ())
        track = video.receiver.track as? RTCVideoTrack

        reliable = pc.dataChannel(forLabel: "input", configuration: RTCDataChannelConfiguration())
        let unordered = RTCDataChannelConfiguration()
        unordered.isOrdered = false
        unordered.maxRetransmits = 0
        fast = pc.dataChannel(forLabel: "input-fast", configuration: unordered)
        reliable?.delegate = events

        try await negotiate(pc)
        guard generation == current, phase != .closed else { throw CancellationError() }
        // Refresh both endpoints before TURN credentials expire, including an idle screen.
        let seconds = max(1, Double(connection.expiresAt) / 1000 - Date().timeIntervalSince1970 - 300)
        renewTask = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(seconds)) } catch { return }
            guard let self, self.generation == current, self.phase != .closed else { return }
            self.phase = .reconnecting
            self.scheduleRecovery(after: .zero)
        }
        connectionTimer = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(30)) } catch { return }
            guard let self, self.generation == current, self.phase != .live else { return }
            self.close()
            self.phase = .failed("Couldn't connect to the remote screen. Check the computer's connection and try again.")
        }
    }

    /// Offer → host → answer with the reserved session’s ICE configuration.
    private func negotiate(_ pc: RTCPeerConnection) async throws {
        let offer = try await pc.offer(for: Self.noConstraints)
        try await pc.setLocalDescription(offer)
        await waitForCandidates(pc)
        try Task.checkCancellation()
        guard self.pc === pc, phase != .closed else { throw CancellationError() }
        let sdp = pc.localDescription?.sdp ?? offer.sdp
        let answer = try await client.screenOffer(sdp: sdp, session: session, display: display?.id)
        try Task.checkCancellation()
        guard self.pc === pc, phase != .closed else { throw CancellationError() }
        session = answer.session
        try await pc.setRemoteDescription(RTCSessionDescription(type: .answer, sdp: answer.sdp))
    }

    private func waitForCandidates(_ pc: RTCPeerConnection) async {
        guard pc.iceGatheringState != .complete else { return }
        let current = generation
        gatherTask?.cancel()
        gatherTask = Task {
            do { try await Task.sleep(for: .seconds(10)) } catch { return }
            guard self.generation == current else { return }
            self.finishGathering()
        }
        await withCheckedContinuation { gathered = $0 }
    }

    fileprivate func finishGathering(generation: Int? = nil) {
        if let generation, generation != self.generation { return }
        gatherTask?.cancel()
        gatherTask = nil
        gathered?.resume()
        gathered = nil
    }

    fileprivate func iceChanged(_ state: RTCIceConnectionState, generation: Int) {
        guard generation == self.generation, phase != .closed else { return }
        switch state {
        case .connected, .completed:
            connectionTimer?.cancel()
            phase = .live
            lastError = nil
        case .disconnected:
            // Often recovers by itself (Wi-Fi ↔ cellular); reconnect if it does not.
            phase = .reconnecting
            scheduleRecovery(after: .seconds(2))
        case .failed:
            phase = .reconnecting
            scheduleRecovery(after: .zero)
        default:
            break
        }
    }

    private func scheduleRecovery(after delay: Duration) {
        recoverTask?.cancel()
        recoverTask = Task { [weak self] in
            var attempt = 0
            try? await Task.sleep(for: delay)
            while let self, !Task.isCancelled, self.phase == .reconnecting {
                do {
                    // Re-evaluate the signaling route after Wi-Fi/cellular changes and fetch fresh ICE credentials.
                    try await self.connect()
                    return
                } catch {
                    guard !Task.isCancelled, self.phase != .closed else { return }
                    log.info("screen reconnect failed: \(error.localizedDescription)")
                    attempt += 1
                    if attempt >= 3 {
                        self.close()
                        self.phase = .failed(error.localizedDescription)
                        return
                    }
                    try? await Task.sleep(for: .seconds(min(Double(attempt) * 2, 10)))
                }
            }
        }
    }

    public func close() {
        recoverTask?.cancel()
        phase = .closed
        if let session {
            let client = client
            Task { try? await client.screenClose(session: session) }
        }
        session = nil
        teardown()
    }

    private func teardown() {
        generation += 1
        renewTask?.cancel()
        connectionTimer?.cancel()
        finishGathering()
        reliable?.close()
        fast?.close()
        pc?.close()
        reliable = nil
        fast = nil
        pc = nil
        events = nil
        track = nil
    }

    // MARK: input

    /// Sends one input event (display points). Pointer moves go unordered: a late move is useless.
    public func send(_ event: [String: Any]) {
        let isMove = event["type"] as? String == "move"
        let channel = isMove ? (fast?.readyState == .open ? fast : reliable) : reliable
        guard let channel, channel.readyState == .open, let data = try? JSONSerialization.data(withJSONObject: event) else { return }
        channel.sendData(RTCDataBuffer(data: data, isBinary: false))
    }

    /// What to stream: a region of the display (points; `nil` = all of it) at the viewer's pixel size.
    public func view(crop: CGRect?, pixels: CGSize) {
        var msg: [String: Any] = ["type": "view", "long": max(pixels.width, pixels.height), "short": min(pixels.width, pixels.height)]
        if let display { msg["display"] = display.id }
        if let crop { msg["crop"] = [crop.minX, crop.minY, crop.width, crop.height] }
        send(msg)
    }

    public func switchDisplay(_ d: ScreenDisplay) {
        display = d
        send(["type": "view", "display": d.id])
    }

    public func pushClipboard(_ text: String) {
        send(["type": "clipboard", "text": text])
    }

    public func takeRemoteClipboard() -> String? {
        defer { remoteClipboard = nil }
        return remoteClipboard
    }

    fileprivate func received(_ data: Data) {
        guard let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        switch msg["type"] as? String {
        case "clipboard": remoteClipboard = msg["text"] as? String
        case "view":
            if let c = msg["crop"] as? [Double], c.count == 4 {
                streamCrop = CGRect(x: c[0], y: c[1], width: c[2], height: c[3])
            } else {
                streamCrop = nil
            }
        case "error": lastError = msg["message"] as? String
        default: break
        }
    }

    private static let noConstraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
}

/// WebRTC delegate callbacks arrive on its signaling thread; this hops them to the main actor.
private final class PeerEvents: NSObject, RTCPeerConnectionDelegate, RTCDataChannelDelegate, Sendable {
    private let owner: WeakBox
    private let generation: Int

    @MainActor init(owner: ScreenSession, generation: Int) {
        self.owner = WeakBox(owner)
        self.generation = generation
    }

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}
    func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        let owner = owner
        let generation = generation
        Task { @MainActor in owner.value?.iceChanged(newState, generation: generation) }
    }

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {
        guard newState == .complete else { return }
        let owner = owner
        let generation = generation
        Task { @MainActor in owner.value?.finishGathering(generation: generation) }
    }

    func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        let data = buffer.data
        let owner = owner
        Task { @MainActor in owner.value?.received(data) }
    }
}

private final class WeakBox: Sendable {
    @MainActor weak var value: ScreenSession?
    @MainActor init(_ value: ScreenSession) { self.value = value }
}
#endif
