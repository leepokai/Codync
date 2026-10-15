import CodyncKit
import SwiftUI

/// Models come from the selected computer's agent, never a bundled provider list.
struct AgentModelPicker: View {
    let backend: String
    @Binding var selection: String?
    /// The selected model's settings (effort, fast…); they belong to one model.
    @Binding var options: [String: String]
    @Environment(BotStore.self) private var store
    @State private var catalog: AgentModels?
    @State private var catalogScope: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var loading = false
    @State private var failure: String?
    @State private var retry = 0
    @State private var searchText: String = ""

    private var requestKey: String {
        "\(store.computer.id)/\(backend)/\(store.connection == .online)/\(retry)"
    }

    /// The agent's own "Default (recommended)" entry is the same choice as leaving the model unset,
    /// so it folds into the single Default row instead of showing twice.
    private var agentDefault: AgentModels.Model? {
        catalog?.models.first { $0.name.lowercased().hasPrefix("default") }
    }

    private var modelChoices: [(String, String)] {
        let available = (catalog?.models ?? []).filter { $0.id != agentDefault?.id }
        let filtered = available.filter { searchText.isEmpty || $0.name.localizedCaseInsensitiveContains(searchText) }
        let current = available.first { $0.id == catalog?.currentModelId }?.name
        var result = [("", agentDefault?.name ?? current.map { "Default · \($0)" } ?? "Default")]
        result += filtered.map { ($0.id, $0.name) }
        // The selected model stays listed while the query hides it, under its name from the full
        // list — the pill reads this label, so a raw id would show there.
        if let selection, !selection.isEmpty, selection != agentDefault?.id,
           !filtered.contains(where: { $0.id == selection }) {
            result.append((selection, available.first { $0.id == selection }?.name ?? selection))
        }
        return result
    }

    private var value: Binding<String> {
        Binding(
            get: { selection == agentDefault?.id ? "" : selection ?? "" },
            set: {
                let model = $0.isEmpty ? nil : $0
                guard model != selection else { return }
                selection = model
                options = [:]
            }
        )
    }

    /// Settings the selected model offers.
    private var modelOptions: [AgentModels.Option] {
        catalog?.models.first { $0.id == value.wrappedValue }?.options ?? []
    }

    private func optionValue(_ id: String) -> Binding<String> {
        Binding(get: { options[id] ?? "" }, set: { options[id] = $0.isEmpty ? nil : $0 })
    }

    private func choices(_ option: AgentModels.Option) -> [(String, String)] {
        let fallback = option.values.first { $0.id == option.default }?.name
        return [("", fallback.map { "Default · \($0)" } ?? "Default")] + option.values.map { ($0.id, $0.name) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text("Model")
                    .foregroundStyle(Palette.text)
                    .fixedSize()
                Spacer(minLength: 0)
                HStack(spacing: 4) {
                    if backend != "custom" { refreshControl }
                    if backend == "custom" || (!loading && catalog?.models.isEmpty != false) {
                        TextField("Default", text: value)
                            .plainTextInput()
                            .multilineTextAlignment(.trailing)
                            .textFieldStyle(.plain)
                            .frame(minWidth: 0, maxWidth: 180)
                            .outlinedPill()
                    } else {
                        SearchableChoicePicker(selection: value, options: modelChoices, title: "Model",
                                               query: $searchText, fitsAvailableWidth: true)
                    }
                }
            }
            ForEach(modelOptions) { option in
                HStack(spacing: 8) {
                    Text(option.name)
                        .foregroundStyle(Palette.text)
                        .fixedSize()
                    Spacer(minLength: 0)
                    ChoicePicker(selection: optionValue(option.id), options: choices(option), fitsAvailableWidth: true)
                }
                .transition(.opacity)
            }
            if backend != "custom", !loading {
                if let failure {
                    Text(failure)
                        .font(.caption)
                        .foregroundStyle(Palette.warning)
                } else if catalog?.models.isEmpty == true {
                    Text("This agent doesn't advertise a model list. Leave Default or enter a model ID.")
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                }
            }
            Text("Changing the model starts a new agent session. Chat history is kept.")
                .font(.caption)
                .foregroundStyle(Palette.secondary)
        }
        .onChange(of: backend) { _, _ in
            selection = nil
            options = [:]
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: modelOptions.map(\.id))
        .task(id: requestKey) { await load() }
    }

    /// Loading and refresh share one fixed slot, so the row never grows for loading.
    private var refreshControl: some View {
        ZStack {
            IconButton(failure == nil ? "Refresh model list" : "Retry model list", systemImage: "arrow.clockwise") {
                retry += 1
            }
            .opacity(loading ? 0 : 1)
            .disabled(loading)
            .accessibilityHidden(loading)
            if loading {
                Spinner(size: 12)
                    .accessibilityLabel("Loading available models")
                    .transition(.opacity)
            }
        }
        .frame(width: 36, height: 36)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: loading)
    }

    @MainActor private func load() async {
        let scope = "\(store.computer.id)/\(backend)"
        if catalogScope != scope {
            catalog = nil
            catalogScope = scope
        }
        failure = nil
        loading = false
        guard backend != "custom" else { return }
        guard store.connection == .online, let client = store.client else {
            failure = "Connect to this computer to load its models. You can still enter a model ID."
            return
        }
        loading = true
        do {
            let response = try await client.agentModels(backend)
            try Task.checkCancellation()
            catalog = response
        } catch {
            guard !Task.isCancelled else { return }
            failure = "Couldn't load models: \(error.localizedDescription)"
        }
        loading = false
    }
}
