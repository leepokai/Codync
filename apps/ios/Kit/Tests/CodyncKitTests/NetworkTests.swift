import Foundation
import Testing
@testable import CodyncKit

private let home = #"{"id":"network","name":"home","computer":"desk","role":"owner","canInvite":true,"canRecover":false,"connector":"public","computers":[{"name":"desk","role":"owner","me":true,"online":true,"agents":[{"id":"reviewer","name":"Reviewer"}],"canRemove":false,"canPromote":false},{"name":"laptop","role":"overlord","me":false,"online":false,"agents":[{"id":"lead","name":"Lead"}],"canRemove":true,"canPromote":true}],"rules":[{"fromComputer":"laptop","fromAgent":"lead","toAgent":"reviewer","access":"allow"},{"fromComputer":"laptop","fromAgent":"*","toAgent":"*","access":"deny"},{"fromComputer":"*","fromAgent":"*","toAgent":"coder","access":"later"}]}"#
private let work = #"{"id":"network-1a2b","name":"work","computer":"desk","role":"member","computers":[],"rules":[]}"#

@Test func decodesEveryNetworkAndItsEvent() throws {
    let view = #"{"joined":true,"networks":[\#(home),\#(work)],"assistants":true,"assistantsAvailable":true}"#
    let decoded = try JSONDecoder().decode(NetworkView.self, from: Data(view.utf8))
    #expect(decoded.joined && decoded.hasContent)
    #expect(decoded.networks?.map(\.name) == ["home", "work"])
    let first = try #require(decoded.networks?.first)
    #expect(first.role == .owner && first.computer == "desk")
    #expect(first.computers?.map(\.name) == ["desk", "laptop"])
    #expect(first.computers?.last?.role == .member, "an unknown role reads as a member")
    #expect(first.rules?.last?.access == .deny, "an unknown access reads as a refusal")

    guard case let .network(event) = HostClient.parseEvent(Data(#"{"type":"network","network":\#(view)}"#.utf8)) else {
        Issue.record("network event"); return
    }
    #expect(event == decoded)
    // Outside a network: shown only while cloud assistants can reach this computer and are on.
    let alone = { (json: String) in try JSONDecoder().decode(NetworkView.self, from: Data(json.utf8)) }
    #expect(try !alone(#"{"joined":false,"networks":[],"assistants":false,"assistantsAvailable":true}"#).hasContent)
    #expect(try !alone(#"{"joined":false,"assistants":true,"assistantsAvailable":false}"#).hasContent, "not in an account")
    #expect(try alone(#"{"joined":false,"assistants":true,"assistantsAvailable":true}"#).hasContent)
    // A network event this app can't read is skipped, never a rewind.
    #expect(HostClient.parseEvent(Data(#"{"type":"network","network":{"joined":"yes"}}"#.utf8)) == nil)
}

@Test func rulesReadWithTheBotsNames() throws {
    let network = try JSONDecoder().decode(NetworkView.Network.self, from: Data(home.utf8))
    let texts = (network.rules ?? []).map { network.text(of: $0) }
    #expect(texts == [
        "Allow: Lead on laptop → Reviewer",
        "Deny: any bot on laptop → any bot here",
        "Deny: any bot on any computer → coder",
    ])
    let only = NetworkView.Rule(fromComputer: "*", fromAgent: "lead", toAgent: "*", access: .deny)
    #expect(network.text(of: only) == "Deny: lead on any computer → any bot here", "never wider than it is")
}

@Test func botsSayWhetherTheNetworkAndCloudAssistantsReachThem() throws {
    let bot = { (extra: String) in
        try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b","name":"Reviewer","backend":"claude","cwd":"/tmp"\#(extra)}"#.utf8))
    }
    let old = try bot("")
    #expect(old.offeredToNetwork == nil && old.reachableByAssistants == nil, "an older host has no switches")
    let new = try bot(#","offeredToNetwork":false,"reachableByAssistants":true"#)
    let draft = BotDraft(new)
    #expect(draft.offeredToNetwork == false && draft.reachableByAssistants == true)
    let sent = try JSONSerialization.jsonObject(with: JSONEncoder().encode(BotDraft(old))) as? [String: Any]
    #expect(sent?["offeredToNetwork"] == nil, "nothing to send to a host without it")
}
