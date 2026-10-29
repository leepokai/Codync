import AVFoundation
import CodyncKit
import SwiftUI

/// Voice chat settings: which voice carries calls (Apple's on this device, or OpenAI / Gemini on the
/// user's own key kept by the computer), the mode, models and voice. Opened from a call's gear and
/// from the computer's row in settings; the computer is the `BotStore` in the environment.
public struct VoiceChatSettingsView: View {
    @AppStorage(VoiceSettings.engineKey) private var engine = "device"
    @AppStorage(VoiceSettings.dictationEngineKey) private var dictationEngine = "device"
    @AppStorage(OnDeviceEngine.rateKey) private var rate = Double(AVSpeechUtteranceDefaultSpeechRate)
    @AppStorage(OnDeviceEngine.languageKey) private var language = ""
    @State private var languages = OnDeviceEngine.languageOptions
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader {
                HStack(spacing: 8) {
                    Image(systemName: "waveform").font(.title2).accessibilityHidden(true)
                    Text(L10n.tr("Voice chat"))
                }
            }
            CardForm {
                CardSection("Voice", footer: engineFooter) {
                    ValueRow("Engine") {
                        ChoicePicker(selection: $engine.animation(Motion.reduced(Motion.layout, reduceMotion)),
                                     options: [("device", "On device")] + VoiceProvider.allCases.map { ($0.rawValue, $0.name) })
                    }
                    if let provider = VoiceProvider(rawValue: engine) {
                        ModeRows(provider: provider).id(provider)
                    } else {
                        ValueRow("Language", detail: "The language you speak in calls.") {
                            ChoicePicker(selection: $language, options: languages)
                        }
                        ValueRow("Reading speed") {
                            ChoicePicker(selection: $rate, options: [
                                (0.42, "Slower"), (Double(AVSpeechUtteranceDefaultSpeechRate), "Normal"), (0.56, "Faster"),
                            ])
                        }
                        PauseRow()
                    }
                }
                if let provider = VoiceProvider(rawValue: engine) {
                    ProviderVoiceSettings(provider: provider).id(provider)
                }
                if let provider = dictationProvider {
                    ProviderVoiceSettings(provider: provider, dictationOnly: true).id("dictation-\(provider.rawValue)")
                }
            }
        }
        .background(Palette.background)
        .tracked(.screenViewed(.voiceSettings))
    }

    /// Dictation on another provider than calls still needs that provider's key and speech-to-text model.
    private var dictationProvider: VoiceProvider? {
        dictationEngine == engine ? nil : VoiceProvider(rawValue: dictationEngine)
    }

    private var engineFooter: String {
        guard let provider = VoiceProvider(rawValue: engine) else {
            return "Apple speech, on this device. During a call, applies when you close settings."
        }
        return "Your own \(provider.name) key, billed by \(provider.name). During a call, applies when you close settings."
    }
}

/// A provider's mode, and the pause that ends what you say when it's Speech.
private struct ModeRows: View {
    @AppStorage private var mode: String
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(provider: VoiceProvider) {
        _mode = AppStorage(wrappedValue: "realtime", VoiceSettings.modeKey(provider))
    }

    var body: some View {
        ValueRow("Mode", detail: mode == "speech" ? "Sent when you pause, replies read aloud." : "A live conversation. Talk over replies.") {
            ChoicePicker(selection: $mode.animation(Motion.reduced(Motion.layout, reduceMotion)),
                         options: [("realtime", "Realtime"), ("speech", "Speech")])
        }
        if mode == "speech" { PauseRow() }
    }
}

/// How long a silence ends what you say, before it's sent.
private struct PauseRow: View {
    @AppStorage(OnDeviceEngine.pauseKey) private var pause = 1.5

    var body: some View {
        ValueRow("Send after a pause of", detail: "Longer leaves time to think mid-sentence.") {
            ChoicePicker(selection: $pause, options: [(1.0, "1 s"), (1.5, "1.5 s"), (2.5, "2.5 s")])
        }
    }
}

/// One provider: mode, the key the computer keeps, models, voice and this month's minutes.
/// `dictationOnly` keeps the key and speech-to-text model.
private struct ProviderVoiceSettings: View {
    let provider: VoiceProvider
    let dictationOnly: Bool
    @Environment(BotStore.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage private var mode: String
    @AppStorage(VoiceSettings.dictationEngineKey) private var dictationEngine = "device"
    /// nil while the computer hasn't answered yet.
    @State private var saved: Bool?
    @State private var replacing = false
    @State private var key = ""
    @State private var busy = false
    @State private var result: (ok: Bool, text: String)?
    @State private var voice = ""
    @State private var model = ""
    @State private var transcribeModel = ""
    @State private var speechModel = ""
    @State private var models = VoiceModels(models: [], transcribe: [], speech: [])

    init(provider: VoiceProvider, dictationOnly: Bool = false) {
        self.provider = provider
        self.dictationOnly = dictationOnly
        _mode = AppStorage(wrappedValue: "realtime", VoiceSettings.modeKey(provider))
    }

    private var speechMode: Bool { mode == "speech" }
    /// Speech mode and dictation with this provider both use its speech-to-text model.
    private var transcribes: Bool { dictationOnly || speechMode || dictationEngine == provider.rawValue }
    private var hasKey: Bool { saved == true }

    var body: some View {
        CardSection(provider.name, footer: footer) {
            VStack(alignment: .leading, spacing: 8) {
                if hasKey && !replacing {
                    savedKeyRow
                } else if Self.canEditKey {
                    keyEntry
                } else if saved == false {
                    missingKeyRow
                }
                if let result {
                    Label(result.text, systemImage: result.ok ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                        .font(.footnote)
                        .foregroundStyle(result.ok ? Palette.secondary : Palette.danger)
                        .transition(.opacity)
                }
            }
            Group {
                if transcribes {
                    ValueRow("Speech to text") { picker($transcribeModel, models.transcribe) }
                }
                if !dictationOnly {
                    if speechMode {
                        ValueRow("Text to speech") { picker($speechModel, models.speech) }
                    } else {
                        ValueRow("Model") { picker($model, models.models) }
                    }
                    ValueRow("Voice") { ChoicePicker(selection: $voice, options: provider.voices.map { ($0, $0) }) }
                    ValueRow("This month", value: "≈ \(VoiceUsage.minutes(for: provider)) min")
                }
            }
            .disabled(!hasKey)
            .opacity(hasKey ? 1 : 0.55)
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: replacing)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: result?.text)

        Color.clear.frame(height: 0)
            .task { await load() }
            .onChange(of: voice) { _, v in UserDefaults.standard.set(v, forKey: VoiceSettings.voiceKey(provider)) }
            .onChange(of: model) { _, m in VoiceSettings.save(m, default: VoiceSettings.defaults(provider).realtime, forKey: VoiceSettings.modelKey(provider)) }
            .onChange(of: transcribeModel) { _, m in
                VoiceSettings.save(m, default: VoiceSettings.defaults(provider).transcribe, forKey: VoiceSettings.transcribeKey(provider))
            }
            .onChange(of: speechModel) { _, m in
                VoiceSettings.save(m, default: VoiceSettings.defaults(provider).speech, forKey: VoiceSettings.speechKey(provider))
            }
    }

    private var footer: String? {
        guard hasKey else { return "Add a key to choose from \(provider.name)'s models." }
        if dictationOnly { return "For dictation in the message box." }
        if provider == .openAI && !speechMode && LiveCallTransport.isModel(model) {
            return "GPT-Live voice and the Responses tool router are billed separately by OpenAI."
        }
        return nil
    }

    /// The key is on the computer: say so, and offer test / replace / remove.
    private var savedKeyRow: some View {
        HStack(spacing: 10) {
            Image(systemName: "key.fill")
                .foregroundStyle(Palette.secondary)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(L10n.tr("Key saved")).foregroundStyle(Palette.text).lineLimit(1)
                Text(L10n.tr("Stays on \(store.hostName)")).font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if busy {
                Spinner()
            } else {
                IconButton("Test key", systemImage: "checkmark.seal", action: test)
                if Self.canEditKey {
                    IconButton("Replace key", systemImage: "pencil") { replacing = true; result = nil }
                    IconButton("Remove key", systemImage: "trash", action: remove)
                }
            }
        }
    }

    /// Keys are entered on the computer itself (the host takes `setVoiceKey` from loopback only).
        private static let canEditKey = false

    /// On the phone: where to add the key.
    private var missingKeyRow: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "key")
                .foregroundStyle(Palette.tertiary)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.tr("No \(provider.name) key on \(store.hostName)")).foregroundStyle(Palette.text)
                Text(L10n.tr("Add it in Codync on that computer: Computers & devices, then Voice chat."))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// Paste a key; the computer checks it with the provider before keeping it.
    private var keyEntry: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                SecureField("API key", text: $key, prompt: Text(provider.keyPrompt))
                    .plainTextInput()
                    .textFieldStyle(.plain)
                    .onSubmit(test)
                if busy {
                    Spinner()
                } else {
                    Button(L10n.tr("Save"), action: test)
                        .buttonStyle(.primary)
                        .disabled(key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            HStack {
                WebLink("Get a key from \(provider.name)", url: provider.keyPage).font(.footnote)
                Spacer()
                if replacing {
                    Button(L10n.tr("Cancel")) { replacing = false; key = ""; result = nil }
                        .buttonStyle(.plain)
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                }
            }
        }
    }

    private func picker(_ selection: Binding<String>, _ list: [String]) -> some View {
        // The current choice stays listed before the provider's list arrives.
        let options = list.contains(selection.wrappedValue) ? list : [selection.wrappedValue] + list
        return ChoicePicker(selection: selection, options: options.map { ($0, $0) })
    }

    private func load() async {
        voice = VoiceSettings.voice(provider)
        model = VoiceSettings.model(provider)
        transcribeModel = VoiceSettings.transcribeModel(provider)
        speechModel = VoiceSettings.speechModel(provider)
        if let data = UserDefaults.standard.data(forKey: VoiceSettings.modelsKey(provider)),
           let cached = try? JSONDecoder().decode(VoiceModels.self, from: data) {
            models = cached
        }
        guard let client = store.client else {
            saved = false
            result = (false, "Not connected to \(store.hostName).")
            return
        }
        do {
            let status = try await client.voiceStatus()
            VoiceSettings.remember(status)
            saved = status.configured(provider)
        } catch {
            saved = false
            result = (false, error.localizedDescription)
            return
        }
        if hasKey { await loadModels(client) }
    }

    private func loadModels(_ client: HostClient) async {
        guard let list = try? await client.voiceModels(provider) else { return }
        models = list
        // A newer default moves every choice the user hasn't made themselves.
        VoiceSettings.remember(list.defaults, for: provider)
        model = VoiceSettings.model(provider)
        transcribeModel = VoiceSettings.transcribeModel(provider)
        speechModel = VoiceSettings.speechModel(provider)
        UserDefaults.standard.set(try? JSONEncoder().encode(list), forKey: VoiceSettings.modelsKey(provider))
    }

    /// A pasted key is checked with the provider by the computer before it's kept. With a key
    /// saved, Test starts a call credential with the chosen model and voice; for dictation only, it
    /// lists the key's models.
    private func test() {
        let typed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !typed.isEmpty || hasKey else { return }
        run { client in
            if typed.isEmpty || !replacing && hasKey {
                if dictationOnly {
                    _ = try await client.voiceModels(provider)
                } else if speechMode {
                    _ = try await client.voiceSpeak(provider, model: speechModel, voice: voice, text: "OK")
                } else {
                    _ = try await client.voiceSession(provider, model: model, voice: voice)
                }
                result = (true, provider == .openAI && !dictationOnly && !speechMode && LiveCallTransport.isModel(model)
                    ? "Model access verified" : "Key works")
            } else {
                saved = try await client.setVoiceKey(typed, for: provider).configured(provider)
                key = ""
                replacing = false
                result = (true, "Key works and is saved on \(store.hostName)")
            }
            await loadModels(client)
        }
    }

    private func remove() {
        run { client in
            saved = try await client.setVoiceKey("", for: provider).configured(provider)
            result = nil
        }
    }

    private func run(_ work: @escaping @MainActor (HostClient) async throws -> Void) {
        guard !busy else { return }
        guard let client = store.client else {
            result = (false, "Not connected to \(store.hostName).")
            return
        }
        busy = true
        result = nil
        Task {
            do {
                try await work(client)
            } catch {
                result = (false, error.localizedDescription)
            }
            busy = false
        }
    }
}
