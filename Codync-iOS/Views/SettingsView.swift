import CodyncKit
import SwiftUI
import UserNotifications

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var notificationsAllowed: Bool?
    @State private var confirmUnpair = false

    var body: some View {
        Form {
            Section("Computer") {
                LabeledContent("Name", value: model.hostName)
                LabeledContent("Status") {
                    switch model.connection {
                    case .online: Text("Connected").foregroundStyle(Palette.accent)
                    case .connecting: Text("Connecting…")
                    case .offline: Text("Offline").foregroundStyle(Palette.warning)
                    case .unpaired: Text("Not paired")
                    }
                }
                if let url = model.client?.baseURL.absoluteString {
                    LabeledContent("Address", value: url).font(.footnote.monospaced())
                }
                if let hello = model.hello {
                    LabeledContent("Host version", value: hello.version)
                    LabeledContent("System", value: hello.os)
                }
            }

            Section {
                ForEach(model.usage.providers) { UsageCard(provider: $0) }
                Button("Refresh") { Task { await model.refreshUsage() } }
            } header: {
                Text("Usage limits")
            } footer: {
                Text("Read on your computer from Claude Code and Codex. Add the Codync widget to your Home or Lock Screen to keep an eye on them.")
            }

            Section("Notifications") {
                if notificationsAllowed == false {
                    Button("Allow in Settings") {
                        UIApplication.shared.open(URL(string: UIApplication.openNotificationSettingsURLString)!)
                    }
                } else if notificationsAllowed == nil {
                    Button("Turn on notifications") {
                        Task { notificationsAllowed = await PushRegistrar.shared.requestAuthorization() }
                    }
                } else {
                    Label("On — you'll hear when a bot needs you or finishes", systemImage: "bell.badge")
                        .font(.footnote)
                }
            }

            if !model.hiddenBots.isEmpty {
                Section("Hidden bots") {
                    ForEach(model.hiddenBots) { bot in
                        HStack {
                            CharacterAvatar(bot: bot, size: 28, animated: false)
                            Text(bot.name)
                            Spacer()
                            Button("Unhide") { model.setHidden(bot, false) }
                        }
                    }
                }
            }

            Section {
                Button("Unpair this phone", role: .destructive) { confirmUnpair = true }
            } footer: {
                Text("Bots and conversations stay on your computer.")
            }

            Section {
                Link("Source code", destination: URL(string: "https://github.com/leepokai/Codync")!)
                LabeledContent("App version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
            }
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
        }
        .confirmationDialog("Unpair from \(model.hostName)?", isPresented: $confirmUnpair, titleVisibility: .visible) {
            Button("Unpair", role: .destructive) {
                model.unpair()
                dismiss()
            }
        }
        .task {
            let status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
            notificationsAllowed = switch status {
            case .authorized, .provisional, .ephemeral: true
            case .denied: false
            default: nil
            }
        }
    }
}

struct UsageCard: View {
    let provider: UsageProvider

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(provider.name).font(.subheadline.bold())
                Spacer()
                Text("updated \(RelativeTime.short(Date(milliseconds: provider.updatedAt)))")
                    .font(.caption2)
                    .foregroundStyle(Palette.tertiary)
            }
            ForEach(provider.windows) { w in
                VStack(alignment: .leading, spacing: 3) {
                    HStack {
                        Text(w.label).font(.caption)
                        Spacer()
                        Text("\(Int(w.percent.rounded()))%").font(.caption.monospacedDigit().bold())
                        if let reset = w.resetDate {
                            Text("· resets in \(RelativeTime.until(reset))").font(.caption2).foregroundStyle(Palette.tertiary)
                        }
                    }
                    UsageBar(percent: w.percent)
                }
            }
        }
        .padding(.vertical, 4)
    }
}

struct UsageBar: View {
    let percent: Double

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.bubbleAgent)
                Capsule()
                    .fill(percent >= 90 ? Palette.danger : percent >= 70 ? Palette.warning : Palette.accentFill)
                    .frame(width: geo.size.width * min(1, max(0.02, percent / 100)))
            }
        }
        .frame(height: 6)
    }
}

/// Compact usage chips at the top of the roster.
struct UsageStrip: View {
    let usage: Usage

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(usage.providers) { p in
                    ForEach(p.windows.prefix(2)) { w in
                        HStack(spacing: 6) {
                            Gauge(value: min(w.percent, 100), in: 0...100) { EmptyView() }
                                .gaugeStyle(.accessoryCircularCapacity)
                                .scaleEffect(0.42)
                                .frame(width: 22, height: 22)
                                .tint(w.percent >= 90 ? Palette.danger : w.percent >= 70 ? Palette.warning : Palette.accent)
                            VStack(alignment: .leading, spacing: 0) {
                                Text("\(p.name) \(w.label)").font(.caption2).foregroundStyle(Palette.tertiary)
                                Text("\(Int(w.percent.rounded()))%").font(.caption.bold().monospacedDigit()).foregroundStyle(Palette.text)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Palette.surface, in: Capsule())
                        .overlay(Capsule().stroke(Palette.border))
                    }
                }
            }
        }
    }
}
