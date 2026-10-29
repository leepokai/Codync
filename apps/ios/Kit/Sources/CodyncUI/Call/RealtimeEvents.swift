import Foundation
@preconcurrency import WebRTC

/// WebRTC callbacks arrive on its signaling thread; this hops them to the main actor.
final class RealtimeEvents: NSObject, RTCPeerConnectionDelegate, RTCDataChannelDelegate, Sendable {
    private let owner: WeakEngine
    private let generation: Int

    @MainActor init(owner: OpenAIRealtimeEngine, generation: Int) { self.owner = WeakEngine(owner); self.generation = generation }

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        let (owner, generation) = (owner, generation)
        Task { @MainActor in owner.value?.iceChanged(newState, generation: generation) }
    }

    func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {
        let (state, channel) = (dataChannel.readyState, ObjectIdentifier(dataChannel))
        let (owner, generation) = (owner, generation)
        Task { @MainActor in
            if state == .open { owner.value?.opened(generation: generation) } else if state == .closed { owner.value?.channelClosed(channel, generation: generation) }
        }
    }

    func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        let data = buffer.data
        let (owner, generation) = (owner, generation)
        Task { @MainActor in owner.value?.received(data, generation: generation) }
    }
}

final class WeakEngine: Sendable {
    @MainActor weak var value: OpenAIRealtimeEngine?
    @MainActor init(_ value: OpenAIRealtimeEngine) { self.value = value }
}
