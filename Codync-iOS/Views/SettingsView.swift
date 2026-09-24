import CodyncKit
import CodyncUI
import SwiftUI
import UserNotifications

struct SettingsView: View {
    @Environment(BotStore.self) private var model
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

