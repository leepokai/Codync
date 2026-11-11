import CodyncKit
import SwiftUI

/// A titled block of rows (the fork's ready-made bots and teams, and the marketplace catalogs).
struct MarketSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.tr(title))
                .font(.headline)
                .foregroundStyle(Palette.text)
                .padding(.leading, 4)
                .accessibilityAddTraits(.isHeader)
            content
        }
    }
}

/// `Weekdays at 09:00`, `Mondays at 08:49`, `Day 1 of each month at 09:00`.
func routineScheduleText(_ routine: MarketAgentRoutine) -> String {
    let time = routine.schedule.time ?? ""
    let days = ["Sundays", "Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays"]
    switch routine.schedule.kind {
    case "weekdays": return L10n.tr("Weekdays at {time}").replacingOccurrences(of: "{time}", with: time)
    case "weekly":
        let day = days[min(max(routine.schedule.weekday ?? 1, 0), 6)]
        return "\(L10n.tr(day)) \(time)"
    case "monthly":
        return L10n.tr("Day {day} of each month at {time}")
            .replacingOccurrences(of: "{day}", with: String(routine.schedule.day ?? 1))
            .replacingOccurrences(of: "{time}", with: time)
    default: return L10n.tr("Daily at {time}").replacingOccurrences(of: "{time}", with: time)
    }
}

/// A marketplace agent's page: its title, the skills it brings, its routines and its instructions.
struct MarketAgentSheet: View {
    let agent: MarketAgent
    let added: Bool
    let onAdded: () -> Void
    let onSkill: (MarketAgentSkill) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var detail: MarketAgentDetail?
    @State private var busy = false
    @State private var error: String?

    private var installed: Bool { added || agent.installed }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(agent.name) {
                if busy {
                    Spinner()
                } else if installed {
                    Image(systemName: "checkmark").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
                } else {
                    IconButton("Add agent", systemImage: "plus") { add() }
                }
            }
            CardForm {
                CardSection {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(agent.title).font(.body).foregroundStyle(Palette.text)
                        Text(L10n.tr("By {name} · Reviewed by Codync").replacingOccurrences(of: "{name}", with: agent.creator))
                            .font(.caption).foregroundStyle(Palette.secondary)
                    }
                    .padding(.vertical, 4)
                }
                if let page = detail {
                    if !page.skills.isEmpty {
                        CardSection(title: "Skills") {
                            ForEach(page.skills) { skill in
                                Button { onSkill(skill) } label: {
                                    HStack(spacing: 12) {
                                        SkillGlyph()
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(skill.name).font(.body).foregroundStyle(Palette.text)
                                            Text(skill.description).font(.caption).foregroundStyle(Palette.secondary).lineLimit(2)
                                        }
                                        Spacer(minLength: 8)
                                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !page.routines.isEmpty {
                        CardSection(title: "Routines") {
                            ForEach(page.routines, id: \.name) { routine in
                                HStack(spacing: 12) {
                                    TileIcon(systemName: "calendar.badge.clock")
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(routine.name).font(.body).foregroundStyle(Palette.text)
                                        Text("\(routineScheduleText(routine)) · \(routine.active ? L10n.tr("Active") : L10n.tr("Inactive"))")
                                            .font(.caption).foregroundStyle(Palette.secondary)
                                    }
                                }
                            }
                        }
                    }
                    CardSection(title: "Information") {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(page.instructions).font(.body).foregroundStyle(Palette.text).textSelection(.enabled)
                            Text(L10n.tr("By {name} · reviewed by Codync · {category} · version {version}")
                                .replacingOccurrences(of: "{name}", with: agent.creator)
                                .replacingOccurrences(of: "{category}", with: L10n.tr(MarketCategories.label(agent.category)))
                                .replacingOccurrences(of: "{version}", with: String(agent.version)))
                                .font(.caption).foregroundStyle(Palette.secondary)
                        }
                    }
                }
                if let error {
                    CardSection { Text(L10n.tr(error)).font(.body).foregroundStyle(Palette.danger) }
                }
            }
        }
        .background(Palette.background)
        .task(id: agent.id) {
            do { detail = try await model.marketAgent(agent.id) } catch { report(error) }
        }
    }

    private func add() {
        busy = true
        Task {
            do {
                try await model.installAgent(agent.id)
                onAdded()
                dismiss()
            } catch { report(error) }
            busy = false
        }
    }

    private func report(_ error: Error) { self.error = error.localizedDescription }
}

/// An app's page: what it does, its setup (steps, a key or a sign-in) and example prompts.
struct AppSheet: View {
    let app: MarketApp
    let added: Bool
    let onAdded: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @Environment(\.openURL) private var openURL
    @State private var values: [String: String] = [:]
    @State private var busy = false
    @State private var error: String?

    private var installed: Bool { added || app.installed }
    private var missing: Bool { app.fields.contains { ($0.required || $0.url) && (values[$0.id] ?? "").isEmpty } }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(app.name) {
                if busy {
                    Spinner()
                } else if installed {
                    Image(systemName: "checkmark").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
                } else {
                    IconButton(app.setup == "link" ? "Connect" : "Add app", systemImage: "plus") { add() }
                        .disabled(missing)
                }
            }
            CardForm {
                CardSection {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(app.tagline).font(.body).foregroundStyle(Palette.text)
                        Text(app.description).font(.body).foregroundStyle(Palette.secondary)
                        if let origin = app.origin, let url = URL(string: origin) {
                            Link("\(L10n.tr("Origin")): \(app.creator)", destination: url).font(.footnote)
                        }
                        if let site = app.website, let url = URL(string: site) {
                            Link(site, destination: url).font(.footnote)
                        }
                    }
                    .padding(.vertical, 4)
                }
                if app.setup == "local", !app.steps.isEmpty {
                    CardSection(title: "Setup") {
                        ForEach(Array(app.steps.enumerated()), id: \.offset) { index, step in
                            Text("\(index + 1). \(L10n.tr(step))").font(.body).foregroundStyle(Palette.text)
                        }
                        if let docs = app.docsUrl, let url = URL(string: docs) {
                            Link(app.docsLabel ?? L10n.tr("Open the guide"), destination: url).font(.footnote)
                        }
                    }
                }
                if app.setup == "key" {
                    CardSection(title: "Setup") {
                        ForEach(app.fields) { field in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(L10n.tr(field.label)).font(.caption).foregroundStyle(Palette.secondary)
                                Group {
                                    if field.url {
                                        TextField(field.placeholder ?? field.label, text: Binding(
                                            get: { values[field.id] ?? "" }, set: { values[field.id] = $0 }))
                                    } else {
                                        SecureField(field.placeholder ?? field.label, text: Binding(
                                            get: { values[field.id] ?? "" }, set: { values[field.id] = $0 }))
                                    }
                                }
                                .plainTextInput()
                                if let hint = field.hint { Text(L10n.tr(hint)).font(.caption).foregroundStyle(Palette.secondary) }
                            }
                        }
                        if let docs = app.docsUrl, let url = URL(string: docs) {
                            Link(app.docsLabel ?? L10n.tr("Get a key"), destination: url).font(.footnote)
                        }
                    }
                }
                if app.setup == "link" || app.setup == "open" {
                    CardSection(title: "Setup") {
                        Text(app.setup == "link"
                             ? L10n.tr("{name} asks you to sign in to your own account in the browser.").replacingOccurrences(of: "{name}", with: app.name)
                             : L10n.tr("Nothing to fill in here."))
                            .font(.body).foregroundStyle(Palette.secondary)
                    }
                }
                if !app.prompts.isEmpty {
                    CardSection(title: "Try it") {
                        ForEach(app.prompts) { prompt in
                            HStack(spacing: 12) {
                                TileIcon(systemName: "text.bubble")
                                Text(prompt.text).font(.body).foregroundStyle(Palette.text)
                            }
                        }
                    }
                }
                if let error {
                    CardSection { Text(L10n.tr(error)).font(.body).foregroundStyle(Palette.danger) }
                }
            }
        }
        .background(Palette.background)
    }

    private func add() {
        busy = true
        Task {
            do {
                let connector = try await model.installApp(app, values: values)
                if connector.needsSignIn {
                    try await ConnectorSignInFlow(model: model, webAuthenticationSession: webAuthenticationSession, openURL: openURL)
                        .signIn(connector.id)
                }
                try await model.ready().verifyConnector(connector.id)
                onAdded()
                dismiss()
            } catch { error = error.localizedDescription }
            busy = false
        }
    }
}
