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
@Test func engineChoiceChangesWithWhatTheCallIsBuiltFrom() {
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
