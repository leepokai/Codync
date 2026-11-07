import Foundation
import Testing
@testable import CodyncKit

private func message(_ file: SharedFile?, marked: Bool?) -> EntryData {
    var data = EntryData(text: "Here's what I found.")
    data.final = true
    data.files = file.map { [$0] }
    data.voiceNote = marked
    return data
}

private let sound = SharedFile(id: "note", name: "note.m4a", size: 120_000, sha256: String(repeating: "0", count: 64))

/// A message marked `voiceNote` plays inside its bubble only when its file is a sound the chat can play; every
/// other file message, marked or not, keeps today's file cards.
@Test func voiceNoteMessagesPickTheBubblePlayerOnlyForPlayableSounds() {
    #expect(message(sound, marked: true).voiceNoteFile == sound)
    let document = SharedFile(id: "doc", name: "notes.txt", size: 10, sha256: String(repeating: "0", count: 64))
    #expect(message(document, marked: true).voiceNoteFile == nil)
    let tooBig = SharedFile(id: "long", name: "long.mp3", size: 21 * 1024 * 1024, sha256: String(repeating: "0", count: 64))
    #expect(message(tooBig, marked: true).voiceNoteFile == nil)
    #expect(message(sound, marked: false).voiceNoteFile == nil)
    #expect(message(sound, marked: nil).voiceNoteFile == nil)
    #expect(message(nil, marked: true).voiceNoteFile == nil)
}

/// The host sends the mark beside the files it belongs to; an older host's message without it decodes unchanged.
@Test func voiceNoteMarkDecodesWithTheFilesItMarks() throws {
    let payload = try JSONSerialization.data(withJSONObject: [
        "text": "On it.",
        "final": true,
        "files": [["id": "note", "name": "note.m4a", "size": 120_000, "sha256": String(repeating: "0", count: 64)]],
        "voiceNote": true,
    ])
    let data = try JSONDecoder().decode(EntryData.self, from: payload)
    #expect(data.voiceNote == true)
    #expect(data.voiceNoteFile == sound)

    let older = try JSONDecoder().decode(EntryData.self, from: Data(#"{"text":"On it.","final":true}"#.utf8))
    #expect(older.voiceNote == nil)
    #expect(older.voiceNoteFile == nil)
}
