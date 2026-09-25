import AppKit
import IOKit.pwr_mgt
@preconcurrency import WebRTC

/// One phone watching: answers its WebRTC offer, streams a display as H.264,
/// and applies input from its data channels (`input` reliable, `input-fast` for moves).
@MainActor
final class Session: NSObject {
    static let factory: RTCPeerConnectionFactory = {
        RTCInitializeSSL()
        return RTCPeerConnectionFactory(encoderFactory: ScreenEncoderFactory(), decoderFactory: RTCDefaultVideoDecoderFactory())
    }()

    let id: String
    private weak var helper: ScreenHelper?
    private var pc: RTCPeerConnection?
    private let track: RTCVideoTrack
    private let capture: Capture
    private var channels: [RTCDataChannel] = []
    private var gathered: CheckedContinuation<Void, Never>?
    private var capturing = false
    private var awake: IOPMAssertionID = 0
    private var clipboardCount = NSPasteboard.general.changeCount
    private var clipboardTask: Task<Void, Never>?

    var display: CGDirectDisplayID { capture.display }

    init(id: String, display: CGDirectDisplayID, helper: ScreenHelper) {
        self.id = id
        self.helper = helper
        let source = Self.factory.videoSource(forScreenCast: true)
        track = Self.factory.videoTrack(with: source, trackId: "screen")
        capture = Capture(display: display, source: source)
        super.init()
        let config = RTCConfiguration()
        config.sdpSemantics = .unifiedPlan
        // No STUN/TURN: the phone already reaches this computer (LAN / Tailscale), so host
        // candidates connect; TCP candidates cover networks that drop UDP.
        config.iceServers = []
        config.tcpCandidatePolicy = .enabled
        config.continualGatheringPolicy = .gatherOnce
        config.bundlePolicy = .maxBundle
        pc = Self.factory.peerConnection(with: config, constraints: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil), delegate: self)
    }

    /// Answers the phone's offer (first connect or an ICE restart). Non-trickle: waits
    /// for local candidates so the answer carries them all.
    func answer(offer: String) async throws -> String {
        guard let pc else { throw HelperError("session closed") }
        try await pc.setRemoteDescription(RTCSessionDescription(type: .offer, sdp: offer))
        if !capturing, let video = pc.transceivers.first(where: { $0.mediaType == .video }) {
            video.sender.track = track
            video.sender.streamIds = ["screen"]
            var error: NSError?
            video.setDirection(.sendOnly, error: &error)
            if let error { throw error }
        }
        let answer = try await pc.answer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil))
        try await pc.setLocalDescription(answer)
        await waitForCandidates()
        if !capturing {
            tune()
            try await capture.start()
            capturing = true
            keepAwake(true)
            watchClipboard()
        }
        return pc.localDescription?.sdp ?? answer.sdp
    }

    /// Screen content: keep text sharp (drop frames before resolution), allow a high bitrate on fast links.
    private func tune() {
        guard let sender = pc?.transceivers.first(where: { $0.mediaType == .video })?.sender else { return }
        let p = sender.parameters
        p.degradationPreference = NSNumber(value: RTCDegradationPreference.maintainResolution.rawValue)
        for e in p.encodings {
            e.maxBitrateBps = 16_000_000
            e.maxFramerate = 60
            e.networkPriority = .high
        }
        sender.parameters = p
    }

    private func waitForCandidates() async {
        guard pc?.iceGatheringState != .complete else { return }
        Task {
            try? await Task.sleep(for: .seconds(3))
            self.finishGathering()
        }
        await withCheckedContinuation { gathered = $0 }
    }

    private func finishGathering() {
        gathered?.resume()
        gathered = nil
    }

    func close() {
        finishGathering()
        clipboardTask?.cancel()
        keepAwake(false)
        channels.forEach { $0.close() }
        channels = []
        pc?.close()
        pc = nil
        let capture = capture
        Task { await capture.stop() }
    }

    // MARK: display wake

    private func keepAwake(_ on: Bool) {
        if on {
            // Wake a sleeping display, then keep it on while someone watches.
            var activity: IOPMAssertionID = 0
            IOPMAssertionDeclareUserActivity("Codync remote screen" as CFString, kIOPMUserActiveLocal, &activity)
            IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleDisplaySleep as CFString,
                                        IOPMAssertionLevel(kIOPMAssertionLevelOn), "Codync remote screen" as CFString, &awake)
        } else if awake != 0 {
            IOPMAssertionRelease(awake)
            awake = 0
        }
    }

    // MARK: clipboard

    private func watchClipboard() {
        clipboardTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                self?.checkClipboard()
            }
        }
    }

    private func checkClipboard() {
        let pb = NSPasteboard.general
        guard pb.changeCount != clipboardCount else { return }
        clipboardCount = pb.changeCount
        if let text = pb.string(forType: .string), text.utf8.count <= 256 * 1024 {
            send(["type": "clipboard", "text": text])
        }
    }

    private func send(_ msg: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: msg),
              let channel = channels.first(where: { $0.label == "input" && $0.readyState == .open })
        else { return }
        channel.sendData(RTCDataBuffer(data: data, isBinary: false))
    }

    // MARK: messages from the phone

    private func received(_ data: Data) {
        guard let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        switch msg["type"] as? String {
        case "clipboard":
            guard let text = msg["text"] as? String else { return }
            let pb = NSPasteboard.general
            pb.clearContents()
            pb.setString(text, forType: .string)
            clipboardCount = pb.changeCount
        case "view":
            let display = (msg["display"] as? NSNumber)?.uint32Value
            let crop = (msg["crop"] as? [NSNumber]).flatMap { c in
                c.count == 4 ? CGRect(x: c[0].doubleValue, y: c[1].doubleValue, width: c[2].doubleValue, height: c[3].doubleValue) : nil
            }
            let capture = capture
            Task {
                do {
                    let applied = try await capture.view(display: display, crop: crop, long: InputInjector.number(msg["long"]), short: InputInjector.number(msg["short"]))
                    // Frames from here on show this region: the phone re-aligns its view on this.
                    let region: Any = applied.crop.map { [$0.minX, $0.minY, $0.width, $0.height] } ?? NSNull()
                    self.send(["type": "view", "display": applied.display, "crop": region])
                } catch {
                    self.send(["type": "error", "message": error.localizedDescription])
                }
            }
        default:
            guard let helper else { return }
            let display = self.display
            Task {
                do {
                    try await helper.input.perform(msg, display: display)
                } catch {
                    self.send(["type": "error", "message": error.localizedDescription])
                }
            }
        }
    }

    private func connectionChanged(_ state: RTCIceConnectionState) {
        switch state {
        case .failed, .closed:
            // `disconnected` can recover (or the phone restarts ICE); `failed` can't.
            helper?.sessionEnded(id)
        default:
            break
        }
    }

    private func opened(_ channel: RTCDataChannel) {
        channel.delegate = self
        channels.append(channel)
    }
}

extension Session: RTCPeerConnectionDelegate, RTCDataChannelDelegate {
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    nonisolated func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}

    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        Task { @MainActor in self.connectionChanged(newState) }
    }

    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {
        guard newState == .complete else { return }
        Task { @MainActor in self.finishGathering() }
    }

    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {
        Task { @MainActor in self.opened(dataChannel) }
    }

    nonisolated func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {}

    nonisolated func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        let data = buffer.data
        Task { @MainActor in self.received(data) }
    }
}

/// The default encoders, except H.264 always runs at level 5.2. WebRTC advertises level 3.1,
/// which VideoToolbox enforces: anything over 1280×720 fails to encode and WebRTC falls back
/// to software VP8. Phones decode any level, so only the encoder needs lifting.
final class ScreenEncoderFactory: NSObject, RTCVideoEncoderFactory {
    private let base = RTCDefaultVideoEncoderFactory()

    func createEncoder(_ info: RTCVideoCodecInfo) -> (any RTCVideoEncoder)? {
        guard info.name == kRTCVideoCodecH264Name else { return base.createEncoder(info) }
        var params = info.parameters
        let profile = RTCH264ProfileLevelId(hexString: params["profile-level-id"] ?? kRTCLevel31ConstrainedBaseline)?.profile ?? .constrainedBaseline
        params["profile-level-id"] = RTCH264ProfileLevelId(profile: profile, level: .level5_2).hexString
        return RTCVideoEncoderH264(codecInfo: RTCVideoCodecInfo(name: info.name, parameters: params))
    }

    func supportedCodecs() -> [RTCVideoCodecInfo] {
        base.supportedCodecs()
    }
}
