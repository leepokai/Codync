import AVFoundation
import CodyncKit
import SwiftUI

/// A voice call with a bot, as Grok Bot does it: a floating bar over the chat, which stays
/// readable underneath. What you say goes out as ordinary messages, its final replies are
/// read aloud, and the chat gets a "Voice chat · 00:16" line when the call ends.
struct CallView: View {
    let botId: String
    @Binding var isSpeaking: Bool
    @Binding var interrupt: (() -> Void)?
    let close: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var session: (any VoiceEngine)?
    @State private var callID = UUID()
    @State private var startedAt = Date.now
    /// When the current cloud engine started, for this month's minutes.
    @State private var cloudStartedAt: Date?
    /// One line under the bar, e.g. why the call fell back to on-device voice.
    @State private var notice: String?
    @State private var settings = false
    /// `voice_call_started` went out for this call.
    @State private var reported = false

    private var bot: Bot? { model.bots[botId] }
    private var color: Color { bot.map { AvatarPalette.color($0.avatarColor) } ?? Palette.accent }

    var body: some View {
        VStack(spacing: 6) {
            bar
            if case .failed(let message) = session?.phase {
                line(message, color: Palette.danger)
            } else if let notice {
                line(notice, color: Palette.secondary)
            }
        }
        .padding(.horizontal, 12)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: session?.phase)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: notice)
        .onAppear {
            startedAt = .now
            begin(cloud: VoiceSettings.provider)
        }
        .onDisappear {
            session?.end()
            countCloudMinutes()
            isSpeaking = false
            interrupt = nil
            model.endVoiceCall(callID)
            model.logCall(botId, seconds: Int(Date.now.timeIntervalSince(startedAt)))
        }
        .onChange(of: session?.phase) { _, phase in
            isSpeaking = phase == .speaking
            interrupt = phase == .speaking ? { session?.interrupt() } : nil
            // Counted once audio flows, so it names the engine the call ended up on.
            if phase?.isActive == true, !reported, let session {
                reported = true
                Analytics.shared.capture(.voiceCallStarted(session.analyticsName))
            }
            // A cloud engine that can't start or drops mid-call hands over to on-device voice.
            if case .failed(let message) = phase, let provider = session?.provider {
                countCloudMinutes()
                notice = "\(provider.name) isn't available (\(message)). Using on-device voice."
                begin(cloud: nil)
            }
        }
        .codyncSheet(isPresented: $settings) { VoiceChatSettingsView() }
    }

    /// Starts the engine: a realtime provider on the key kept by the computer, or on-device speech.
    private func begin(cloud provider: VoiceProvider?) {
        // Picks up the computer's newest models for the next call; this one starts right away.
        if provider != nil, let client = model.client {
            Task { if let status = try? await client.voiceStatus() { VoiceSettings.remember(status) } }
        }
        let op = CallOperator(model: model, botId: botId)
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
                send: { [model, botId] in model.send($0, to: botId) }
            ))
            cloudStartedAt = .now
        } else if let provider {
            let chosen = VoiceSettings.model(provider)
            let voice = VoiceSettings.voice(provider)
            let credential: @MainActor () async throws -> String = { [model] in
                guard let client = model.client else { throw VoiceError("Not connected to \(model.hostName).") }
                return try await client.voiceSession(provider, model: chosen, voice: voice)
            }
            engine = switch provider {
            case .openAI: OpenAIRealtimeEngine(credential: credential, op: op)
            case .gemini: GeminiLiveEngine(model: chosen, voice: voice, credential: credential, op: op)
            }
            cloudStartedAt = .now
        } else {
            engine = OnDeviceEngine { [model, botId] in model.send($0, to: botId) }
        }
        engine.onActivityChanged = { [weak engine, model, botId, callID] active in
            if active {
                model.beginVoiceCall(callID, botId: botId,
                                     speak: { [weak engine] in engine?.speak(reply: $0) },
                                     announce: { [weak engine] in engine?.announce($0) },
                                     end: { [weak engine] in engine?.end() })
            } else {
                model.endVoiceCall(callID)
            }
        }
        session = engine
        Task { await engine.start() }
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
                          working: bot?.isWorking == true, color: color, reduceMotion: reduceMotion)
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
            round(muted ? "Unmute" : "Mute", muted ? "mic.slash" : "mic", on: muted) { session?.muted.toggle() }
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
        case .speaking: "Speaking"
        case .failed: "Can't listen"
        case .listening where session?.muted == true: "Muted"
        case .listening: bot?.isWorking == true ? "Working, listening" : "Listening"
        }
    }
}
