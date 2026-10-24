import CodyncKit

/// A command in the composer's `/` list.
struct SlashCommand: Identifiable, Equatable {
    enum Action: Hashable { case fast, skill(String) }

    /// What follows the `/`.
    let name: String
    let icon: String
    let title: String
    let detail: String
    let warning: Bool
    let action: Action

    var id: Action { action }
}

/// The composer's `/` commands, listed while the draft is just `/partial`, the same in a group: Fast mode runs here
/// through the host's `updateBot` (a group's on each member; a new session is in the bot's menu); a skill fills in `/<id> `, and the host turns that message into "read this skill first"
/// (`host/src/market/skills.rs` `expand_command`). docs/features/marketplace.md, "Commands".
enum SlashCommands {
    /// The command name being typed after a leading `/`, before any space.
    static func query(_ draft: String) -> String? {
        guard draft.hasPrefix("/"), !draft.contains(where: \.isWhitespace) else { return nil }
        return String(draft.dropFirst())
    }

    /// Built-in commands, then the skills the bot has on (a group's: any member's), whose name or
    /// title holds `query`. `members` are a group's bots.
    static func list(for bot: Bot?, members: [Bot] = [], query: String, skills: [InstalledSkill]) -> [SlashCommand] {
        guard let bot else { return [] }
        let owners = bot.isGroup ? members : [bot]
        let own = skills.filter { skill in owners.contains { $0.skills.contains(skill.id) } }.map {
            SlashCommand(name: $0.id, icon: "book.pages", title: $0.name, detail: $0.description, warning: false, action: .skill($0.id))
        }
        return (builtIn(bot, members: members) + own).filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) || $0.title.localizedCaseInsensitiveContains(query)
        }
    }

    /// A draft that is just a built-in command (`/fast`): sending it runs the command.
    static func builtIn(for bot: Bot?, members: [Bot] = [], draft: String) -> SlashCommand? {
        guard let bot, let name = query(draft.trimmingCharacters(in: .whitespacesAndNewlines)) else { return nil }
        return builtIn(bot, members: members).first { $0.name == name }
    }

    /// Fast mode, while the bot (or a member of the group) shows it.
    private static func builtIn(_ bot: Bot, members: [Bot]) -> [SlashCommand] {
        let fast: (detail: String, state: Bot.FastModeState)
        if bot.isGroup {
            let group = GroupFastMode(members)
            guard !group.members.isEmpty else { return [] }
            fast = (group.detail, group.state)
        } else {
            guard bot.fastModeShown else { return [] }
            fast = (bot.fastModeDetail, bot.fastModeState)
        }
        return [SlashCommand(name: "fast", icon: "bolt", title: "Fast mode", detail: fast.detail,
                             warning: fast.state == .refused, action: .fast)]
    }
}
