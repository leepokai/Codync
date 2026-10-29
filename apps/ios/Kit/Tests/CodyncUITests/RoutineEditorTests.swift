import CodyncKit
import Foundation
import Testing
@testable import CodyncUI

@Test func routineEditorWaitsForItsRoutineToBeListed() throws {
    let listing = try JSONDecoder().decode(RoutineListing.self, from: Data(#"{"routines":[{"id":"r","botId":"b","name":"Check","instruction":"Check","triggers":[],"enabled":false,"createdAt":0,"updatedAt":0,"triggerDescriptions":[]}],"runs":[]}"#.utf8))
    #expect(!EditorTarget(id: "1", routineId: "r").isReady(in: []))
    #expect(EditorTarget(id: "1", routineId: "r").isReady(in: listing.routines))
    #expect(!EditorTarget(id: "1", routineId: "other").isReady(in: listing.routines))
    #expect(EditorTarget(id: "1", routineId: nil).isReady(in: []))
}
