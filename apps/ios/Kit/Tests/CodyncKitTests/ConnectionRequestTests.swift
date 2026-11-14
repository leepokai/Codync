import Foundation
import Testing
@testable import CodyncKit

@Test func tokenConnectorsOfferSignInOnlyWhenTheHostSaysSo() throws {
    let older = #"{"id":"github","name":"GitHub","description":"","kind":"remote","keys":["Authorization"],"auth":"none"}"#
    #expect(try JSONDecoder().decode(InstalledConnector.self, from: Data(older.utf8)).canSignIn != true)
    let newer = #"{"id":"github","name":"GitHub","description":"","kind":"remote","keys":["Authorization"],"auth":"none","canSignIn":true}"#
    #expect(try JSONDecoder().decode(InstalledConnector.self, from: Data(newer.utf8)).canSignIn == true)
    let plan = #"{"url":"https://github.com/login/device","callback":"host","code":"ABCD-1234"}"#
    #expect(try JSONDecoder().decode(ConnectorSignIn.self, from: Data(plan.utf8)).code == "ABCD-1234")
}

@Test func secureConnectionRequestIsChatVisibleWithoutCredentialValues() throws {
    let wire = #"{"id":"request","seq":1,"botId":"bot","rev":1,"kind":"notice","turn":1,"createdAt":0,"updatedAt":0,"data":{"text":"Connect the service","connectionRequest":{"kind":"secret","status":"pending","title":"Example","botId":"bot","connectorId":"example","field":"API_KEY","location":"env"}}}"#
    let entry = try JSONDecoder().decode(Entry.self, from: Data(wire.utf8))
    #expect(entry.isChat)
    #expect(entry.data.connectionRequest?.field == "API_KEY")
    #expect(entry.data.connectionRequest?.status == "pending")
    let encoded = try JSONEncoder().encode(entry)
    let object = try JSONSerialization.jsonObject(with: encoded) as? [String: Any]
    let data = object?["data"] as? [String: Any]
    let request = data?["connectionRequest"] as? [String: Any]
    #expect(request?["value"] == nil)
}
