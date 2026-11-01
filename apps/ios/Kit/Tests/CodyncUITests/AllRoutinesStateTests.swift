import CodyncKit
import Foundation
import Testing
@testable import CodyncUI

private func routine(_ id: String, name: String) throws -> Routine {
    try JSONDecoder().decode(Routine.self, from: Data(#"{"id":"\#(id)","botId":"a","name":"\#(name)","instruction":"","triggers":[],"enabled":true,"createdAt":0,"updatedAt":0,"triggerDescriptions":[]}"#.utf8))
}

/// A routine just saved shows at once: a new one joins the list, an edited one replaces its old self.
@MainActor @Test func aSavedRoutineShowsBeforeTheNextListing() throws {
    let state = AllRoutinesState()
    state.saved(try routine("1", name: "Daily"))
    state.saved(try routine("2", name: "Weekly"))
    state.saved(try routine("1", name: "Morning"))
    #expect(state.routines.map(\.name) == ["Morning", "Weekly"])
}
