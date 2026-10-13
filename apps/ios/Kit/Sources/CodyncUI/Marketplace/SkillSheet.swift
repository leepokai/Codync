import CodyncKit
import SwiftUI

/// The skill a details sheet shows: an installed one, or one from the catalog.
struct SkillRef: Identifiable {
    let id: String
    let name: String
    let description: String
    /// A catalog skill's folder in anthropics/skills; `nil` for an installed skill.
    var source: String?
    var installed = true

    init(_ skill: InstalledSkill) {
        (id, name, description) = (skill.id, skill.name, skill.description)
    }

    init(_ skill: MarketSkill, installed: Bool) {
        (id, name, description, source, self.installed) = ("catalog/\(skill.source)", skill.name, skill.description, skill.source, installed)
    }

    /// What the host's `skillDetails` looks it up by.
    var request: [String: String] { source.map { ["source": $0] } ?? ["id": id] }
}

/// A skill's details (desktop `SkillSheet`): what it tells the bot, its `SKILL.md`, with Add
/// while it isn't installed.
struct SkillSheet: View {
    let skill: SkillRef
    @Environment(BotStore.self) private var model
    @State private var markdown: String?
    @State private var error: String?
    @State private var adding = false

    private var added: Bool {
        guard let source = skill.source else { return true }
        return skill.installed || model.installedSkills.contains { $0.id == source.lowercased() }
    }

    /// Older hosts list skills but can't show what's in them.
    private var readable: Bool { !AppVersion.isBelow(model.hello?.version ?? "", SkillCommand.minHost) }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(skill.name) {
                if added {
                    Image(systemName: "checkmark")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 32)
                        .accessibilityLabel("Added")
                } else if adding {
                    Spinner()
                } else {
                    IconButton("Add", systemImage: "plus") { add() }
                }
            }
            CardForm {
                if !skill.description.isEmpty {
                    CardSection {
                        Text(skill.description).font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                }
                if readable {
                    CardSection("Instructions", footer: added ? "Bots read these before using the skill." : "Add it to let bots use these instructions.") {
                        if let markdown {
                            if markdown.isEmpty {
                                Text("No instructions.").foregroundStyle(Palette.secondary)
                            } else {
                                MarkdownText(markdown)
                            }
                        } else if error == nil {
                            Spinner()
                        }
                    }
                }
                if let error {
                    CardSection {
                        Text(error).font(.footnote).foregroundStyle(Palette.danger)
                    }
                }
            }
        }
        .background(Palette.background)
        .task {
            guard readable else { return }
            do {
                markdown = try await model.skillDetails(skill)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func add() {
        guard let source = skill.source else { return }
        adding = true
        error = nil
        Task {
            do {
                try await model.installSkill(source: source)
            } catch {
                self.error = error.localizedDescription
            }
            adding = false
        }
    }
}
