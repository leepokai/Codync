import SwiftUI
import Testing
@testable import CodyncUI

@MainActor @Suite struct MenuRowsTests {
    @Test func rowsAreEqualWhenTheyShowTheSameThingWhateverTheirActions() {
        #expect(MenuItem("Logs", icon: "doc.text") { print("one") } == MenuItem("Logs", icon: "doc.text") { print("two") })
    }

    @Test func anyChangeARowShowsMakesItDifferent() {
        let row = MenuItem("Logs", icon: "doc.text") {}
        #expect(row != MenuItem("Files", icon: "doc.text") {})
        #expect(row != MenuItem("Logs", icon: "trash") {})
        #expect(row != MenuItem("Logs", icon: "doc.text", selected: true) {})
        #expect(row != MenuItem("Logs", icon: "doc.text", destructive: true) {})
        #expect(row != MenuItem("Logs", icon: "doc.text", divider: true) {})
        #expect(row != MenuItem("Logs", icon: "doc.text", enabled: false) {})
        #expect(MenuItem("Move", submenu: [MenuItem("A") {}]) != MenuItem("Move", submenu: [MenuItem("B") {}]))
    }

    @Test func aRowThatDidNotUpdateRunsItsNewestAction() {
        var ran: [String] = []
        let latest = LatestMenuRows()
        _ = latest.holding([MenuItem("Logs") { ran.append("old") }, MenuItem("Move", submenu: [MenuItem("A") { ran.append("old A") }])])
        _ = latest.holding([MenuItem("Logs") { ran.append("new") }, MenuItem("Move", submenu: [MenuItem("A") { ran.append("new A") }])])
        latest.run(MenuItem("Logs") {}, row: 0)
        latest.run(MenuItem("A") {}, row: 1, inSubmenu: 0)
        #expect(ran == ["new", "new A"])
    }

    @Test func aRowGoneFromTheMenuRunsNothing() {
        var ran = false
        let latest = LatestMenuRows()
        _ = latest.holding([MenuItem("Logs") { ran = true }])
        latest.run(MenuItem("Logs") {}, row: 3)
        latest.run(MenuItem("Logs") {}, row: 0, inSubmenu: 3)
        #expect(!ran)
    }

    @Test func aPressThatOutranAnUpdateRunsWhatWasPressedNotWhatTookItsPlace() {
        var ran: [String] = []
        let stop = MenuItem("Stop") { ran.append("stop") }
        let first = MenuItem("First") { ran.append("first") }
        let latest = LatestMenuRows()
        _ = latest.holding([MenuItem("Logs") {}, MenuItem("Restart") {}, stop, MenuItem("Move", submenu: [first])])
        _ = latest.holding([
            MenuItem("Logs") {}, MenuItem("Start") {}, MenuItem("Remove") { ran.append("remove") },
            MenuItem("Move", submenu: [MenuItem("Second") { ran.append("second") }]),
        ])
        latest.run(stop, row: 2)
        latest.run(first, row: 3, inSubmenu: 0)
        #expect(ran == ["stop", "first"])
    }
}
