import CodyncKit
import SwiftUI

struct LeadAgentAvatar: View {
    let bot: Bot
    @State private var showingRole = false

    var body: some View {
        CharacterAvatar(bot: bot, size: 72)
            .overlay(alignment: .bottomTrailing) {
                if bot.lead {
                    Button { Motion.animate { showingRole = true } } label: {
                        LeadAgentBadge(size: 28)
                            .frame(width: 44, height: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .padding(-8)
                    .accessibilityLabel(L10n.tr("About the lead agent"))
                }
            }
            .codyncSheet(isPresented: $showingRole) { LeadAgentInfo() }
    }
}

private struct LeadAgentInfo: View {
    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Lead agent")
            ScrollView {
                Text(L10n.tr("Your lead agent is your first point of contact. It follows your goals and unfinished tasks, suggests useful next steps, and coordinates work with your other bots. The lead Codync sets up has a Check in routine that runs twice a day by default; you can change or pause it in Routines."))
                    .font(.body)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
            }
        }
        .background(Palette.background)
    }
}
