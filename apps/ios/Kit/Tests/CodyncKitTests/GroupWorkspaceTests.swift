import Foundation
import Testing
@testable import CodyncKit

// A group's shared workspace folder: the bot payload carries it, old hosts' hellos don't say it's there,
// and the draft sends a path, explicit null or nothing.

@Test func aGroupDecodesItsSharedFolder() throws {
    let json = #"""
    {"id":"g","kind":"group","name":"Crew","description":"","members":["a","b"],"workspace":"/tmp/team"}
    """#
    let group = try JSONDecoder().decode(Bot.self, from: Data(json.utf8))
    #expect(group.workspace == "/tmp/team")
    let bare = #"""
    {"id":"g","kind":"group","name":"Crew","description":"","members":["a"]}
    """#
    let older = try JSONDecoder().decode(Bot.self, from: Data(bare.utf8))
    #expect(older.workspace == nil, "older hosts and groups without one both read as none")
}

@Test func helloSaysWhetherGroupsCarryAFolder() throws {
    let hello = try JSONDecoder().decode(Hello.self, from: Data(#"""
    {"hostId":"h","name":"Mac","version":"2.50.0","os":"macos","backends":[],"rev":1,"groupWorkspace":true}
    """#.utf8))
    #expect(hello.groupWorkspace == true)
    let older = try JSONDecoder().decode(Hello.self, from: Data(#"""
    {"hostId":"h","name":"Mac","version":"2.49.0","os":"macos","backends":[],"rev":1}
    """#.utf8))
    #expect(older.groupWorkspace == nil, "older hosts: editors keep the option hidden")
}

@Test func aGroupDraftSendsAFolderAClearOrNothing() throws {
    let body = { (configure: (inout GroupDraft) -> Void) -> [String: Any] in
        var draft = GroupDraft(name: "Crew", members: ["a"])
        draft.id = "g"
        configure(&draft)
        let data = try JSONEncoder().encode(draft)
        return try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    }
    let untouched = try body { _ in }
    #expect(untouched["workspace"] == nil, "nil isn't sent, so the saved one stays")
    let set = try body { $0.workspace = "/tmp/team" }
    #expect(set["workspace"] as? String == "/tmp/team")
    let cleared = try body { $0.clearWorkspace = true }
    #expect(cleared["workspace"] is NSNull, "clearing is an explicit null")
}
