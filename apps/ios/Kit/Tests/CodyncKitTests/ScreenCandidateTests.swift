import Foundation
import Testing
@testable import CodyncKit

@Test func oldScreenPreparationDefaultsToCompleteSDP() throws {
    let data = Data(#"{"session":"s","iceServers":[],"expiresAt":123}"#.utf8)
    let connection = try JSONDecoder().decode(ScreenConnection.self, from: data)
    #expect(!connection.trickle)
    let updated = Data(#"{"session":"s","iceServers":[],"expiresAt":123,"trickle":true}"#.utf8)
    let trickle = try JSONDecoder().decode(ScreenConnection.self, from: updated)
    #expect(trickle.trickle)
}

@Test func candidateWireFormatPreservesMediaAndCompletion() throws {
    let candidate = ScreenCandidate(candidate: "candidate:1 1 UDP 1 192.0.2.1 5000 typ relay", sdpMLineIndex: 1, sdpMid: "video")
    let data = try JSONEncoder().encode(candidate)
    let roundtrip = try JSONDecoder().decode(ScreenCandidate.self, from: data)
    #expect(roundtrip == candidate)
    try roundtrip.validate()
    let completed = try JSONDecoder().decode(ScreenCandidate.self, from: Data(#"{"type":"complete"}"#.utf8))
    #expect(completed == .complete)
}

@Test func malformedScreenCandidatesAreRejected() throws {
    for json in [
        #"{"type":"candidate","sdpMLineIndex":0}"#,
        #"{"type":"candidate","candidate":"candidate:1\nline","sdpMLineIndex":0}"#,
        #"{"type":"candidate","candidate":"candidate:1","sdpMLineIndex":-1}"#,
        #"{"type":"candidate","candidate":"candidate:1","sdpMLineIndex":17}"#
    ] {
        let event = try JSONDecoder().decode(ScreenCandidate.self, from: Data(json.utf8))
        #expect(throws: HostError.self) { try event.validate() }
    }
}
