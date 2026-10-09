import Foundation
import Testing
@testable import CodyncKit

struct ScreenAccessTests {
    @Test func oldHostScreenStatusStillDecodes() throws {
        let state = try JSONDecoder().decode(ScreenState.self, from: Data("{\"enabled\":true}".utf8))
        #expect(state.permissionApp == nil)
        #expect(!state.available)
        #expect(state.accessGuidance.contains("Settings → Computer access"))
    }

    @Test func guidanceUsesTheActualPermissionAppAndPlatform() {
        var state = ScreenState()
        state.enabled = true
        state.connected = true
        state.permissionApp = "CodyncDevScreen"
        #expect(state.accessGuidance.contains("CodyncDevScreen"))
        state.capture = true
        #expect(state.available)
        #expect(!state.controlReady)
        #expect(state.accessGuidance.contains("Device Control and Data Access"))
        state.platform = "linux"
        #expect(state.accessGuidance.contains("system sharing dialog"))
        state.platform = "windows"
        #expect(state.accessGuidance.contains("not available on Windows"))
        state.computerUse = true
        #expect(state.controlReady)
    }
}
