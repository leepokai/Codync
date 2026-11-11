import Foundation
import Testing
@testable import CodyncKit

@Test func whatsappOutputsKeepRecipientAndSenderSeparateAcrossEncoding() throws {
    var output = RoutineOutput(kind: "whatsapp", target: "5215555555555@s.whatsapp.net")
    #expect(output.missingField != nil)
    output.account = "5215555555556@s.whatsapp.net"
    #expect(output.missingField == nil)
    let restored = try JSONDecoder().decode(RoutineOutput.self, from: JSONEncoder().encode(output))
    #expect(restored == output)
    #expect(restored.account != restored.target)
    #expect(RoutineOutput.kinds.contains { $0.id == "whatsapp" })
}

@Test func routineOutputsAreOptionalForOlderHosts() throws {
    let old = Data(#"{"id":"r","botId":"b","name":"Check","instruction":"Check","triggers":[],"enabled":true,"createdAt":0,"updatedAt":0,"triggerDescriptions":[]}"#.utf8)
    #expect(try JSONDecoder().decode(Routine.self, from: old).outputs == nil)
    var object = try #require(JSONSerialization.jsonObject(with: old) as? [String: Any])
    object["outputs"] = [["kind":"whatsapp", "target":"5215555555555@s.whatsapp.net", "account":"5215555555556@s.whatsapp.net"]]
    let current = try JSONSerialization.data(withJSONObject: object)
    let routine = try JSONDecoder().decode(Routine.self, from: current)
    #expect(routine.outputs?.first?.account == "5215555555556@s.whatsapp.net")
}
