import CodyncKit

/// `/skill` in the composer: while the draft is just `/partial`, the bot's skills are offered.
/// The host turns a message starting with `/<skill id>` into "read this skill first"
/// (`host/src/market/skills.rs` `expand_command`).
enum SkillCommand {
    /// The first host that understands `/skill` messages; older ones get the text as typed.
    static let minHost = "2.14.1"

    /// The skill name being typed after a leading `/`, before any space.
    static func query(_ draft: String) -> String? {
        guard draft.hasPrefix("/"), !draft.contains(where: \.isWhitespace) else { return nil }
        return String(draft.dropFirst())
    }

    /// The bot's skills (ids in `enabled`) whose id or name holds `query`.
    static func matches(_ installed: [InstalledSkill], enabled: [String], query: String) -> [InstalledSkill] {
        installed.filter {
            enabled.contains($0.id)
                && (query.isEmpty || $0.id.localizedCaseInsensitiveContains(query) || $0.name.localizedCaseInsensitiveContains(query))
        }
    }
}
