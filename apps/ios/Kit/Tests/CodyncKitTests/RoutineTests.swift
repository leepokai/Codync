import Foundation
import Testing
@testable import CodyncKit

@Test func routinesDecodeMixedTriggersAndRunHistory() throws {
    let data = Data(#"{"routines":[{"id":"r","botId":"b","name":"Check","instruction":"Check status","triggers":[{"type":"cron","expression":"20 14 * * 0","timeZone":"Asia/Taipei"},{"type":"webhook"}],"enabled":true,"createdAt":0,"updatedAt":0,"triggerDescriptions":["Every Sunday","When a webhook fires"],"nextRunAt":1000}],"runs":[{"id":"run","routineId":"r","botId":"b","status":"pending","createdAt":0,"event":{"source":"schedule"}}]}"#.utf8)
    let listing = try JSONDecoder().decode(RoutineListing.self, from: data)
    #expect(listing.routines[0].triggers[0].timeZone == "Asia/Taipei")
    #expect(listing.routines[0].triggers[1].type == "webhook")
    #expect(listing.runs[0].isActive)
}

@Test func routineTranscriptLinkSurvivesRoundTrip() throws {
    let data = Data(#"{"text":"Created routine","routineId":"r","runId":"run","style":"info"}"#.utf8)
    let entry = try JSONDecoder().decode(EntryData.self, from: data)
    let restored = try JSONDecoder().decode(EntryData.self, from: JSONEncoder().encode(entry))
    #expect(restored.routineId == "r")
    #expect(restored.runId == "run")
}

@Test func routineScheduleResponseCarriesServerPreviewWithoutClientRules() throws {
    let data = Data(#"{"draft":{"kind":"cron","amount":"1","unit":3600,"calendarStyle":"days","weekday":1,"selectedDays":[1,3,5],"monthDay":1,"minuteStep":15,"hour":23,"minute":47,"at":0,"expression":"47 23 * * 1,3,5","zone":"Asia/Taipei","original":[]},"triggers":[{"type":"cron","expression":"47 23 * * 1,3,5","timeZone":"Asia/Taipei"}],"summary":"Server schedule description","nextRunAt":1790470800000,"warning":null}"#.utf8)
    let preview = try JSONDecoder().decode(RoutineSchedulePreview.self, from: data)
    #expect(preview.summary == "Server schedule description")
    #expect(preview.draft.selectedDays == [1, 3, 5])
    #expect(preview.nextRunAt == 1_790_470_800_000)
    let encoded = try JSONEncoder().encode(preview.draft)
    #expect(try JSONDecoder().decode(RoutineScheduleDraft.self, from: encoded) == preview.draft)
}

@Test func routineDateControlPreservesWireMilliseconds() {
    var draft = RoutineScheduleDraft()
    draft.at = 1_779_000_000_001
    let date = draft.date
    draft.date = date
    #expect(draft.at == 1_779_000_000_001)
}

@Test func routineTimeOfDayRoundTripsHourAndMinute() throws {
    var draft = RoutineScheduleDraft()
    draft.hour = 23
    draft.minute = 47
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = .gmt
    #expect(calendar.dateComponents([.hour, .minute], from: draft.timeOfDay) == DateComponents(hour: 23, minute: 47))
    draft.timeOfDay = try #require(calendar.date(from: DateComponents(year: 2030, month: 3, day: 9, hour: 6, minute: 5)))
    #expect(draft.hour == 6)
    #expect(draft.minute == 5)
}

@Test func routineWebhookDecodesWithAndWithoutTheCloud() throws {
    let cloud = Data(#"{"url":"https://api.codync.dev/v1/hooks/c/r","localUrl":"http://127.0.0.1:19222/hooks/routines/r","key":"k","connected":true}"#.utf8)
    let local = Data(#"{"url":null,"localUrl":"http://127.0.0.1:19222/hooks/routines/r","key":"k","connected":false}"#.utf8)
    #expect(try JSONDecoder().decode(RoutineWebhook.self, from: cloud).url == "https://api.codync.dev/v1/hooks/c/r")
    let off = try JSONDecoder().decode(RoutineWebhook.self, from: local)
    #expect(off.url == nil)
    #expect(off.localUrl.hasSuffix("/hooks/routines/r"))
}

@Test func routineTimeoutReadsAsADuration() {
    #expect(Routine.timeoutText("3600") == "1 hour")
    #expect(Routine.timeoutText("7200") == "2 hours")
    #expect(Routine.timeoutText("1800") == "30 minutes")
    #expect(Routine.timeoutText("60") == "1 minute")
    #expect(Routine.timeoutText("90") == "90 seconds")
    #expect(Routine.timeoutText("1") == "1 second")
    #expect(Routine.timeoutText("1.5") == "1.5s")
    #expect(Routine.timeoutText("") == "s")
}

@Test func timeZoneSearchIsFuzzyAndRanksWordStartsFirst() {
    let zones = ["America/New_York", "Asia/Taipei", "Asia/Tokyo", "Europe/Berlin", "America/Argentina/Buenos_Aires"]
    #expect(TimeZones.search("taipei", in: zones) == ["Asia/Taipei"])
    #expect(TimeZones.search("TAIPEI", in: zones) == ["Asia/Taipei"])
    #expect(TimeZones.search("new york", in: zones) == ["America/New_York"])
    #expect(TimeZones.search("new_york", in: zones) == ["America/New_York"])
    #expect(TimeZones.search("asia tai", in: zones) == ["Asia/Taipei"])
    #expect(TimeZones.search("buenos-aires", in: zones) == ["America/Argentina/Buenos_Aires"])
    // A name that starts with the query, then word starts, then text inside a word, then letters in order.
    #expect(TimeZones.search("a", in: zones) == ["America/Argentina/Buenos_Aires", "America/New_York", "Asia/Taipei", "Asia/Tokyo"])
    #expect(TimeZones.search("ai", in: zones) == ["America/Argentina/Buenos_Aires", "Asia/Taipei", "America/New_York", "Asia/Tokyo"])
    #expect(TimeZones.search("nyk", in: zones) == ["America/New_York"])
    #expect(TimeZones.search("  ", in: zones) == zones)
    #expect(TimeZones.search("xyz", in: zones).isEmpty)
    #expect(TimeZones.offset("Asia/Taipei") == "GMT+8")
}

@Test func routineRepeatKeepsTheFiveChoicesAndOpensTheRestAsCustom() {
    #expect(RoutineScheduleDraft.repeatChoices.map(\.id) == ["hourly", "daily", "weekdays", "weekly", "custom"])
    var draft = RoutineScheduleDraft()
    for style in ["hourly", "daily", "weekdays", "weekly", "custom"] {
        draft.calendarStyle = style
        #expect(draft.forMobileEditor().calendarStyle == style)
    }
    for style in ["minutes", "days", "monthly"] {
        draft.calendarStyle = style
        #expect(draft.forMobileEditor().calendarStyle == "custom")
    }
    for (kind, shown) in [("cron", "cron"), ("webhook", "webhook"), ("interval", "keep"), ("once", "keep")] {
        draft.kind = kind
        #expect(draft.forMobileEditor().kind == shown)
    }
}

@Test func customStartsFromTheCronTheHostCompiled() {
    var draft = RoutineScheduleDraft()
    draft.calendarStyle = "weekdays"
    draft.expression = "0 8 * * *"
    #expect(draft.repeating("custom", compiled: "0 9 * * 1-5").expression == "0 9 * * 1-5")
    #expect(draft.repeating("custom", compiled: nil).expression == "")
    #expect(draft.repeating("daily", compiled: "0 9 * * 1-5").expression == "0 8 * * *")
    #expect(draft.repeating("weekdays", compiled: "x") == draft)
    let cron = RoutineTrigger(type: "cron", expression: "0 9 * * 1-5", timeZone: "UTC")
    #expect(RoutineSchedulePreview(draft: draft, triggers: [cron], summary: "").cronExpression == "0 9 * * 1-5")
    #expect(RoutineSchedulePreview(draft: draft, triggers: [RoutineTrigger(type: "webhook")], summary: "").cronExpression == nil)
}

private func bot(_ id: String, kind: String = "agent") throws -> Bot {
    try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"\#(id)","kind":"\#(kind)","name":"\#(id)"}"#.utf8))
}

private func routine(_ id: String, botId: String, name: String) throws -> Routine {
    try JSONDecoder().decode(Routine.self, from: Data(#"{"id":"\#(id)","botId":"\#(botId)","name":"\#(name)","instruction":"","triggers":[],"enabled":true,"createdAt":0,"updatedAt":0,"triggerDescriptions":[]}"#.utf8))
}

@Test func allRoutinesGroupUnderTheirBotInRosterOrderByNameAndUnknownBotsStayOut() throws {
    let sections = Routine.sections(
        [try routine("1", botId: "b", name: "Zeta"), try routine("2", botId: "a", name: "Daily"),
         try routine("3", botId: "b", name: "Alpha"), try routine("4", botId: "gone", name: "Lost")],
        bots: [try bot("b"), try bot("a"), try bot("c")])
    #expect(sections.map(\.bot.id) == ["b", "a"])
    #expect(sections.map { $0.routines.map(\.name) } == [["Alpha", "Zeta"], ["Daily"]])
}

@Test func onlyAgentsCanOwnANewRoutine() throws {
    #expect(Routine.owners([try bot("a"), try bot("g", kind: "group")]).map(\.id) == ["a"])
}
