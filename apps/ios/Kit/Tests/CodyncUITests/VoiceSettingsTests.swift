import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// The computer's newest models become the defaults; a model the user picked stays theirs,
/// and picking the default stores nothing, so the next newer default reaches them too.
@Test @MainActor func voiceDefaultsFollowTheComputerUnlessPicked() throws {
    let p = VoiceProvider.gemini
    let keys = [VoiceSettings.defaultsKey(p), VoiceSettings.transcribeKey(p)]
    defer { keys.forEach(UserDefaults.standard.removeObject(forKey:)) }
    keys.forEach(UserDefaults.standard.removeObject(forKey:))

    #expect(VoiceSettings.transcribeModel(p) == p.transcribeModel)

    let status = try JSONDecoder().decode(VoiceStatus.self, from: Data("""
    {"providers":[{"provider":"gemini","configured":true,
      "defaults":{"realtime":"gemini-9-live","transcribe":"gemini-9-flash","speech":"gemini-9-flash-tts"}}]}
    """.utf8))
    VoiceSettings.remember(status)
    #expect(VoiceSettings.transcribeModel(p) == "gemini-9-flash")

    VoiceSettings.save("gemini-9-flash", default: VoiceSettings.defaults(p).transcribe, forKey: VoiceSettings.transcribeKey(p))
    #expect(UserDefaults.standard.string(forKey: VoiceSettings.transcribeKey(p)) == nil)

    VoiceSettings.save("gemini-1-flash", default: VoiceSettings.defaults(p).transcribe, forKey: VoiceSettings.transcribeKey(p))
    VoiceSettings.remember(VoiceDefaults(realtime: "a", transcribe: "gemini-10-flash", speech: "b"), for: p)
    #expect(VoiceSettings.transcribeModel(p) == "gemini-1-flash")

    // A host from before 2.6.3 sends no defaults: the last known ones stay.
    let old = try JSONDecoder().decode(VoiceStatus.self, from: Data(#"{"providers":[{"provider":"gemini","configured":true}]}"#.utf8))
    VoiceSettings.remember(old)
    #expect(VoiceSettings.defaults(p).transcribe == "gemini-10-flash")
}

/// A call takes another engine only when what its engine is built from changes: the engine, the mode,
/// a model or the voice it uses, or the on-device language.
@Test @MainActor func engineChoiceChangesWithWhatTheCallIsBuiltFrom() {
    let p = VoiceProvider.openAI
    let keys = [OnDeviceEngine.languageKey, VoiceSettings.modelKey(p), VoiceSettings.voiceKey(p), VoiceSettings.modeKey(p),
                VoiceSettings.transcribeKey(p), VoiceSettings.speechKey(p)]
    defer { keys.forEach(UserDefaults.standard.removeObject(forKey:)) }
    keys.forEach(UserDefaults.standard.removeObject(forKey:))

    let device = VoiceSettings.engineChoice(nil)
    #expect(VoiceSettings.engineChoice(nil) == device)
    UserDefaults.standard.set("ja-JP", forKey: OnDeviceEngine.languageKey)
    #expect(VoiceSettings.engineChoice(nil) != device)
    #expect(VoiceSettings.engineChoice(p) != VoiceSettings.engineChoice(.gemini))

    var seen = [VoiceSettings.engineChoice(p)]
    for (key, value) in [(VoiceSettings.modelKey(p), "realtime-9"), (VoiceSettings.voiceKey(p), "cedar"), (VoiceSettings.modeKey(p), "speech"),
                         (VoiceSettings.transcribeKey(p), "transcribe-9"), (VoiceSettings.speechKey(p), "tts-9")] {
        UserDefaults.standard.set(value, forKey: key)
        let choice = VoiceSettings.engineChoice(p)
        #expect(VoiceSettings.engineChoice(p) == choice)
        #expect(!seen.contains(choice), "\(key)")
        seen.append(choice)
    }
}

/// The computer's speech server is offered beside the mic's dictation engines only, never among call engines.
@Test @MainActor func theComputersSpeechServerIsDictationOnly() {
    defer { UserDefaults.standard.removeObject(forKey: VoiceSettings.dictationEngineKey) }
    #expect(DictationEngine.local.pref == "local")

    // Call engines are `VoiceProvider`s (the call engine picker's options): "local" isn't one.
    #expect(VoiceProvider(rawValue: DictationEngine.local.pref) == nil)
    #expect(VoiceProvider.allCases.map(\.rawValue) == ["openai", "gemini"])

    // The dictation pref stores it as "local"; picking it needs no key or model rows.
    UserDefaults.standard.set("local", forKey: VoiceSettings.dictationEngineKey)
    #expect(VoiceSettings.dictationEngine == .local)
}

/// The speech server card's check says what the computer answered (linked or not), that the
/// computer can't be asked while there's no client, and the error when it can't be reached.
@Test @MainActor func theSpeechServerCheckSaysWhatTheComputerAnswers() async {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.connecting)
    let store = BotStore(computer: Computer(id: "voice", name: "Mac", signKey: ""), storage: storage) { _ in remote }
    defer { store.retire() }
    let status = LocalDictationStatus()

    await status.load(on: store)
    #expect(status.error == "Not connected to Mac." && status.local == nil)

    store.client = HostClient(transport: remote)
    await remote.setCanned("localDictation", #"{"ready":false,"message":"Link a model."}"#)
    await status.load(on: store)
    #expect(status.local?.ready == false && status.local?.reason == "Link a model." && status.error == nil)

    await remote.setCanned("localDictation", #"{"ready":true,"message":null}"#)
    await status.load(on: store)
    #expect(status.local?.ready == true && status.error == nil)

    await remote.setCanned("localDictation", throwing: HostError.unreachable)
    await status.load(on: store)
    #expect(status.error == HostError.unreachable.localizedDescription && status.local == nil)
}

@Test @MainActor func googleSearchOnlyChangesGeminiRealtimeEngineChoice() {
    let keys = [VoiceSettings.googleSearchKey, VoiceSettings.modeKey(.gemini)]
    keys.forEach(UserDefaults.standard.removeObject(forKey:))
    defer { keys.forEach(UserDefaults.standard.removeObject(forKey:)) }
    #expect(!VoiceSettings.googleSearch)
    let gemini = VoiceSettings.engineChoice(.gemini)
    let openai = VoiceSettings.engineChoice(.openAI)
    UserDefaults.standard.set(true, forKey: VoiceSettings.googleSearchKey)
    #expect(VoiceSettings.engineChoice(.gemini) != gemini)
    #expect(VoiceSettings.engineChoice(.openAI) == openai)
    UserDefaults.standard.set("speech", forKey: VoiceSettings.modeKey(.gemini))
    let speech = VoiceSettings.engineChoice(.gemini)
    UserDefaults.standard.set(false, forKey: VoiceSettings.googleSearchKey)
    #expect(VoiceSettings.engineChoice(.gemini) == speech)
}

/// The computer says which server the OpenAI engine's Speech mode goes to (Gemini, a cloud computer and an older
/// host have none to choose). One of the user's own leaves Speech as that engine's only mode, on a device that never
/// opened the settings too, and the mode picked before comes back with OpenAI.
@Test @MainActor func aServerOfTheUsersOwnLeavesSpeechAsItsEnginesOnlyMode() throws {
    let keys = VoiceProvider.allCases.flatMap { [VoiceSettings.serverKey($0), VoiceSettings.modeKey($0)] }
    let saved = keys.map { UserDefaults.standard.object(forKey: $0) }
    defer { for (key, value) in zip(keys, saved) { UserDefaults.standard.set(value, forKey: key) } }
    for key in keys { UserDefaults.standard.removeObject(forKey: key) }
    let status = { (server: String) in
        try JSONDecoder().decode(VoiceStatus.self, from: Data("""
        {"providers":[{"provider":"openai","configured":true,"server":"\(server)"},{"provider":"gemini","configured":false}]}
        """.utf8))
    }
    VoiceSettings.remember(try status("http://127.0.0.1:8890"))
    #expect(UserDefaults.standard.string(forKey: VoiceSettings.serverKey(.openAI)) == "http://127.0.0.1:8890")
    #expect(UserDefaults.standard.string(forKey: VoiceSettings.serverKey(.gemini)) == nil)
    #expect(VoiceSettings.isSpeechMode(.openAI))
    #expect(!VoiceSettings.isSpeechMode(.gemini))
    VoiceSettings.remember(try status(""))
    #expect(!VoiceSettings.isSpeechMode(.openAI))
    #expect(VoiceStatus.serverLabel("") == "api.openai.com")
    #expect(VoiceStatus.serverLabel("http://127.0.0.1:8890") == "http://127.0.0.1:8890")
}

/// Another server has other models: the ones picked for the server before go, and the same server keeps them.
@Test @MainActor func anotherServerDropsTheModelsPickedForTheOneBefore() throws {
    let p = VoiceProvider.openAI
    let keys = [VoiceSettings.serverKey(p), VoiceSettings.speechKey(p), VoiceSettings.modelsKey(p)]
    let saved = keys.map { UserDefaults.standard.object(forKey: $0) }
    defer { for (key, value) in zip(keys, saved) { UserDefaults.standard.set(value, forKey: key) } }
    let status = { (server: String) in
        try JSONDecoder().decode(VoiceStatus.self, from: Data(#"{"providers":[{"provider":"openai","configured":true,"server":"\#(server)"}]}"#.utf8))
    }
    VoiceSettings.remember(try status(""))
    UserDefaults.standard.set("tts-9", forKey: VoiceSettings.speechKey(p))
    UserDefaults.standard.set(Data("{}".utf8), forKey: VoiceSettings.modelsKey(p))
    VoiceSettings.remember(try status(""))
    #expect(VoiceSettings.speechModel(p) == "tts-9")
    VoiceSettings.remember(try status("http://127.0.0.1:8890"))
    #expect(VoiceSettings.speechModel(p) == VoiceSettings.defaults(p).speech)
    #expect(UserDefaults.standard.object(forKey: VoiceSettings.modelsKey(p)) == nil)
}
