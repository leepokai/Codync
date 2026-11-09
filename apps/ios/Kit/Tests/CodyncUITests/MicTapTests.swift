import AVFoundation
import Testing
@testable import CodyncUI

/// A microphone tap runs at the rate the hardware runs at: after the route changed while a reply was read aloud,
/// the node still names the old rate, and a tap at it ends the app. Formats only, no audio engine or session
/// (CoreAudio deadlocks test runs).
@Test func micTapFollowsTheHardwareRate() throws {
    let node = try #require(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2))
    #expect(try MicTap.format(node: node, hardwareRate: 48000) === node)

    let tap = try MicTap.format(node: node, hardwareRate: 16000)
    #expect(tap.sampleRate == 16000)
    #expect(tap.channelCount == 2)
    #expect(tap.commonFormat == node.commonFormat)
    #expect(tap.isInterleaved == node.isInterleaved)

    // No usable input: the node or the hardware is at 0 Hz.
    #expect(throws: MicTap.Unavailable.self) { try MicTap.format(node: node, hardwareRate: 0) }
    #expect(throws: MicTap.Unavailable.self) { try MicTap.format(node: AVAudioFormat(), hardwareRate: 48000) }
}
