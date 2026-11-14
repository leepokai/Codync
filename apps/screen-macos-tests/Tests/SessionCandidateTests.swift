import Foundation
import Testing
@testable import ScreenLink

/// Parsed the way `HostLink` parses the host's requests.
private func wire(_ json: String) throws -> [String: Any] {
    try #require(JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
}

private let relay = #"{"type":"candidate","candidate":"candidate:1 1 UDP 1 192.0.2.1 5000 typ relay","sdpMLineIndex":0}"#

@Test func completionFollowsAllCandidatesAndBoundsAreEnforced() throws {
    var progress = CandidateProgress()
    let candidate = try SessionCandidate(json: wire(relay))
    for _ in 0..<CandidateProgress.maxCount { try progress.accept(candidate) }
    #expect(throws: HelperError.self) { try progress.accept(candidate) }
    try progress.accept(.complete)
    #expect(throws: HelperError.self) { try progress.accept(candidate) }
    #expect(throws: HelperError.self) { try progress.accept(.complete) }
}

@Test func wireCandidatesAreValidatedBeforeTheyReachThePeer() throws {
    let long = "candidate:" + String(repeating: "a", count: 4096)
    let rejected = [
        #"{"type":"candidate","candidate":"","sdpMLineIndex":0}"#,
        #"{"type":"candidate","candidate":" ","sdpMLineIndex":0}"#,
        #"{"type":"candidate","candidate":"candidate:1\r\n","sdpMLineIndex":0}"#,
        #"{"type":"candidate","candidate":"\#(long)","sdpMLineIndex":0}"#,
        #"{"type":"candidate","candidate":"candidate:1","sdpMLineIndex":17}"#,
        #"{"type":"candidate","candidate":"candidate:1"}"#,
        #"{"type":"ready"}"#,
        "{}",
    ]
    for json in rejected {
        let value = try wire(json)
        #expect(throws: HelperError.self) { try SessionCandidate(json: value) }
    }
    var withMid = try wire(relay)
    withMid["sdpMid"] = "0"
    let parsed = try SessionCandidate(json: withMid)
    #expect(parsed == .candidate(sdp: "candidate:1 1 UDP 1 192.0.2.1 5000 typ relay", index: 0, mid: "0"))
    #expect(parsed.json["sdpMid"] as? String == "0")
    #expect(try SessionCandidate(json: wire(#"{"type":"complete"}"#)) == .complete)
    #expect(SessionCandidate.complete.json["type"] as? String == "complete")
}
