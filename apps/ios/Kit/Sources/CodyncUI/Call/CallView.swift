import AVFoundation
import CodyncKit
import SwiftUI

/// A voice call with a bot, as Grok Bot does it: a floating bar over the chat, which stays
/// readable underneath. What you say goes out as messages of the call, its final replies are
/// read aloud, and the call ends as one titled line ("Release plan · 00:16") that opens its conversation.
struct CallView: View {
    let botId: String
    @Binding var isSpeaking: Bool
    @Binding var interrupt: (() -> Void)?
    let close: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var session: (any VoiceEngine)?
    @State private var callID = UUID()
    /// The call on the computer, which runs it; set as the call appears.
    @State private var link: CallLink?
    @State private var startedAt = Date.now
    /// When the current cloud engine started, for this month's minutes.
    @State private var cloudStartedAt: Date?
    /// The call settings the current engine was built from.
    @State private var engineChoice = ""
    /// One line under the bar, e.g. why the call fell back to on-device voice.
    @State private var notice: String?
    @State private var settings = false
    /// `voice_call_started` went out for this call.
    @State private var reported = false
    /// Each engine's `voice_call_quality`.
    @State private var quality = CallQuality()
    /// The bot's Live Activity showing the call.
    @State private var activity: CallActivity?

    private var bot: Bot? { model.bots[botId] }
    private var call: String { callID.uuidString }
    private var color: Color { bot.map { AvatarPalette.color($0.avatarColor) } ?? Palette.accent }

    var body: some View {
        VStack(spacing: 6) {
            bar
            if let result = session?.searchResult { CallSearchView(result: result) }
            if case .failed(let message, _) = session?.phase {
                line(message, color: Palette.danger)
            } else if session?.phase == .interrupted {
                Button { session?.resume() } label: {
                    line("Audio paused by another app. Tap to resume.", color: Palette.secondary)
                }
                .buttonStyle(.plain)
            } else if let notice {
                line(notice, color: Palette.secondary)
            }
        }
        .padding(.horizontal, 12)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: session?.phase)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: notice)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: session?.searchResult?.html)
        .onAppear {
            startedAt = .now
            link = CallLink(model: model, botId: botId, call: call)
            if let bot {
                let ref = BotReference(accountId: model.storage.accountID, computerId: model.computer.id, botId: botId)
                activity = CallActivity(bot: bot, computerId: model.computer.id, link: model.storage.botURL(ref),
                                        toggleMute: { session?.muted.toggle() }, hangUp: { close() })
            }
            begin(cloud: VoiceSettings.provider)
        }
        .onDisappear {
            // First, so the connection outlives the engine: the end is sent after the engine's last lines.
            link?.end(seconds: Int(Date.now.timeIntervalSince(startedAt)))
            session?.end()
            countCloudMinutes()
            isSpeaking = false
            interrupt = nil
            model.endVoiceCall(callID)
            quality.report()
            activity?.end()
        }
        .onChange(of: session?.phase) { _, phase in
            if let phase {
                quality.phase(phase)
                activity?.phase(phase)
            }
            isSpeaking = phase == .speaking
            interrupt = phase == .speaking ? { session?.interrupt() } : nil
            // Counted once audio flows, so it names the engine the call ended up on.
            if phase?.isActive == true, !reported, let session {
                reported = true
                Analytics.shared.capture(.voiceCallStarted(session.analyticsName))
                model.callStarted(botId, call: call)
            }
            // A cloud engine that can't start or drops mid-call hands over to on-device voice.
            if case .failed(let message, _) = phase, let provider = session?.provider {
                countCloudMinutes()
                notice = "\(provider.name) isn't available (\(message)). Using on-device voice."
                begin(cloud: nil)
            }
        }
        .onChange(of: session?.muted) { _, muted in activity?.muted(muted == true) }
        .onChange(of: settings) { _, open in if !open { switchEngine() } }
        .codyncSheet(isPresented: $settings) { VoiceChatSettingsView() }
    }

    /// Starts the engine: a realtime provider on the key kept by the computer, or on-device speech.
    private func begin(cloud provider: VoiceProvider?) {
        // Picks up the computer's newest models for the next call; this one starts right away.
        if provider != nil, let client = model.client {
            Task { if let status = try? await client.voiceStatus() { VoiceSettings.remember(status) } }
        }
        engineChoice = VoiceSettings.engineChoice(provider)
        guard let link else { return }
        let engine: any VoiceEngine
        if let provider, VoiceSettings.isSpeechMode(provider) {
            let transcribe = VoiceSettings.transcribeModel(provider)
            let speechModel = VoiceSettings.speechModel(provider)
            let voice = VoiceSettings.voice(provider)
            let client = { [model] () throws -> HostClient in
                guard let client = model.client else { throw VoiceError("Not connected to \(model.hostName).") }
                return client
            }
            engine = CloudSpeechEngine(provider: provider, speech: .init(
                transcribe: { try await VoiceSettings.inReadersScript(client().voiceTranscribe(provider, model: transcribe, wav: $0)) },
                speak: { try await client().voiceSpeak(provider, model: speechModel, voice: voice, text: $0) },
                send: { [model, botId, call] in model.send($0, to: botId, call: call) }
            ))
            // The computer keeps the call, though it carries no voice model.
            Task { _ = try? await link.start("speech", provider: provider) }
            cloudStartedAt = .now
        } else if let provider {
            let chosen = VoiceSettings.model(provider)
            let voice = VoiceSettings.voice(provider)
            engine = switch provider {
            case .openAI: OpenAIRealtimeEngine(model: chosen, voice: voice, link: link)
            case .gemini: GeminiLiveEngine(model: chosen, voice: voice, link: link)
            }
            cloudStartedAt = .now
        } else {
            engine = OnDeviceEngine { [model, botId, call] in model.send($0, to: botId, call: call) }
            Task { _ = try? await link.start("device") }
        }
        quality.begin(engine.analyticsName)
        (engine as? GeminiLiveEngine)?.onStall = { [quality] in quality.stalled() }
        engine.onActivityChanged = { [weak engine, model, callID] active in
            if active {
                model.beginVoiceCall(callID, hear: { [weak engine, activity] cue in
                    engine?.hear(cue)
                    activity?.heard(cue)
                }, end: { [weak engine] in engine?.end() })
            } else {
                model.endVoiceCall(callID)
            }
        }
        // A muted call stays muted on the engine that takes over.
        engine.muted = session?.muted == true
        session = engine
        Task { await engine.start() }
    }

    /// After the call settings close: a different engine picked there takes over the call, which goes on.
    private func switchEngine() {
        let provider = VoiceSettings.provider
        guard VoiceSettings.engineChoice(provider) != engineChoice else { return }
        countCloudMinutes()
        session?.end()
        notice = nil
        begin(cloud: provider)
    }

    private func countCloudMinutes() {
        guard let provider = session?.provider, let since = cloudStartedAt else { return }
        VoiceUsage.add(seconds: Int(Date.now.timeIntervalSince(since)), for: provider)
        cloudStartedAt = nil
    }

    private func line(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.footnote)
            .foregroundStyle(color)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .glass(in: Capsule())
            .transition(.opacity)
    }

    private var bar: some View {
        HStack(spacing: 10) {
            avatar
            LevelDots(level: session?.level ?? 0, speaking: session?.phase == .speaking,
                      working: bot?.isWorking == true, connecting: session?.phase.isConnecting ?? true,
                      color: color, reduceMotion: reduceMotion)
                .frame(minWidth: Self.dotsWidth, maxWidth: Self.dotsMaxWidth, minHeight: Self.dotsHeight, maxHeight: Self.dotsHeight)
                .accessibilityLabel(statusText)
            // Audio leaves the phone: say where.
            if let provider = session?.provider {
                Text(provider.name)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Palette.secondary)
                    .accessibilityLabel("Voice by \(provider.name)")
            }
            round("Call settings", "gearshape") { settings = true }
            let muted = session?.muted == true
            round(muted ? "Unmute" : "Mute", muted ? "mic.slash" : "mic", on: muted) {
                session?.muted.toggle()
            }
            Button(action: close) {
                Image(systemName: "xmark")
                    .font(.system(size: Self.iconSize, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: Self.buttonSize, height: Self.buttonSize)
                    .background(Palette.danger, in: Circle())
            }
            .buttonStyle(PressScale())
            .accessibilityLabel("End call")
            .help("End call")
        }
        .padding(.leading, 14)
        .padding(.trailing, 7)
        .padding(.vertical, 7)
            .glass(in: Capsule())
            .shadow(color: .black.opacity(0.08), radius: 16, y: 6)
    }

    /// The bot; tapping it while it talks cuts the reply short.
    private var avatar: some View {
        Button { session?.interrupt() } label: {
            Group {
                if let bot { CharacterAvatar(bot: bot, size: 34) }
            }
            .scaleEffect(session?.phase == .speaking && !reduceMotion ? 1.1 : 1)
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.6).repeatForever(autoreverses: true), value: session?.phase == .speaking)
        }
        .buttonStyle(.plain)
        .disabled(session?.phase != .speaking)
        .accessibilityLabel(session?.phase == .speaking ? "Interrupt" : bot?.name ?? "")
    }

    private func round(_ label: String, _ symbol: String, on: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: Self.iconSize, weight: .medium))
                .foregroundStyle(on ? Palette.onAccent : Self.buttonInk)
                .contentTransition(.symbolEffect(.replace))
                .frame(width: Self.buttonSize, height: Self.buttonSize)
                .background(on ? Palette.accentFill : Self.buttonFill, in: Circle())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel(label)
        .help(label)
    }

    // The bar spans the screen.
    private static let buttonSize: CGFloat = 46
    private static let iconSize: CGFloat = 17
    private static let dotsHeight: CGFloat = 20
    private static let dotsWidth: CGFloat = 0
    private static let dotsMaxWidth: CGFloat = .infinity
        private static let buttonFill = Palette.background
        private static let buttonInk = Palette.text

    private var statusText: String {
        switch session?.phase {
        case .starting, nil: "Connecting"
        case .reconnecting: "Reconnecting"
        case .interrupted: "Paused"
        case .speaking: "Speaking"
        case .failed: "Can't listen"
        case .listening where session?.muted == true: "Muted"
        case .listening: bot?.isWorking == true ? "Working, listening" : "Listening"
        }
    }
}
