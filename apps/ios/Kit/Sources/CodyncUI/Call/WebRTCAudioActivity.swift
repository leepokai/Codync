import Foundation
@preconcurrency import WebRTC

/// Live has no response-finished event. The native receiver's measured audio drives the speaking indicator.
@MainActor
final class WebRTCAudioActivity {
    private var task: Task<Void, Never>?

    init(peer: RTCPeerConnection, changed: @escaping @MainActor (Bool, Float) -> Void) {
        task = Task { [weak peer] in
            var lastSound = ContinuousClock.now - .seconds(1)
            while !Task.isCancelled, let peer {
                let levels: (Double, Double) = await withCheckedContinuation { continuation in
                    peer.statistics { report in
                        var input = 0.0
                        var output = 0.0
                        for stat in report.statistics.values {
                            let level = (stat.values["audioLevel"] as? NSNumber)?.doubleValue ?? 0
                            if stat.type == "inbound-rtp" { output = max(output, level) }
                            if stat.type == "media-source" { input = max(input, level) }
                        }
                        continuation.resume(returning: (input, output))
                    }
                }
                guard !Task.isCancelled else { return }
                if levels.1 > 0.005 { lastSound = .now }
                changed(lastSound.duration(to: .now) < .milliseconds(450), Float(min(1, levels.0 * 5)))
                do { try await Task.sleep(for: .milliseconds(150)) } catch { return }
            }
        }
    }

    func stop() { task?.cancel(); task = nil }
}
