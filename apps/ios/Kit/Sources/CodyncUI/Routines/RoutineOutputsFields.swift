import CodyncKit
import SwiftUI

struct RoutineOutputsFields: View {
    @Binding var outputs: [RoutineOutput]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n.tr("Outputs")).font(.headline)
            ForEach(outputs.indices, id: \.self) { index in
                if outputs.indices.contains(index) {
                    let output = outputs[index]
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            ChoicePicker(selection: text(index, \.kind), options: RoutineOutput.kinds)
                                .accessibilityLabel(L10n.tr("Output kind"))
                            Spacer(minLength: 8)
                            Button(L10n.tr("Remove")) {
                                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                                    guard outputs.indices.contains(index) else { return }
                                    outputs.remove(at: index)
                                }
                            }.buttonStyle(.secondary)
                        }
                        Field("Destination") {
                            TextField(output.destinationPlaceholder, text: text(index, \.target))
                                .outputInput().accessibilityLabel(L10n.tr("Destination"))
                        }
                        if output.kind == "telegram" || output.kind == "whatsapp" {
                            Field(output.kind == "whatsapp" ? "Linked WhatsApp account" : "Telegram bot") {
                                TextField(output.kind == "whatsapp"
                                          ? "Your linked number, e.g. 5215555555556@s.whatsapp.net"
                                          : L10n.tr("The bot it sends as, if there are several"),
                                          text: optionalText(index, \.account)).outputInput()
                            }
                        }
                        if ["githubLabel", "githubAssign", "githubReview", "githubIssue"].contains(output.kind) {
                            Field(valueLabel(output.kind)) {
                                TextField(L10n.tr(valuePlaceholder(output.kind)), text: optionalText(index, \.value)).outputInput()
                            }
                        }
                        if let problem = output.missingField {
                            Text(L10n.tr(problem)).font(.caption).foregroundStyle(Palette.danger)
                        }
                    }
                }
            }
            if outputs.count < 10 {
                Button(L10n.tr("Add an output")) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { outputs.append(RoutineOutput()) }
                }.buttonStyle(.secondary)
            }
            Text(L10n.tr("Sent once, after the last step, with its reply. Empty is fine."))
                .font(.caption).foregroundStyle(Palette.secondary)
        }
    }

    private func text(_ index: Int, _ key: WritableKeyPath<RoutineOutput, String>) -> Binding<String> {
        Binding(get: { outputs.indices.contains(index) ? outputs[index][keyPath: key] : "" }, set: { value in
            guard outputs.indices.contains(index) else { return }
            outputs[index][keyPath: key] = value
        })
    }

    private func optionalText(_ index: Int, _ key: WritableKeyPath<RoutineOutput, String?>) -> Binding<String> {
        Binding(get: { outputs.indices.contains(index) ? outputs[index][keyPath: key] ?? "" : "" }, set: { value in
            guard outputs.indices.contains(index) else { return }
            outputs[index][keyPath: key] = value
        })
    }

    private func valueLabel(_ kind: String) -> String {
        kind == "githubIssue" ? "Issue title" : kind == "githubLabel" ? "Labels" : "People"
    }

    private func valuePlaceholder(_ kind: String) -> String {
        kind == "githubIssue" ? "Issue title (optional)" : kind == "githubLabel" ? "Labels, separated by commas" : "People, separated by commas"
    }
}

private extension View {
    func outputInput() -> some View {
        textFieldStyle(.plain).textInputAutocapitalization(.never).autocorrectionDisabled()
            .padding(12).background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}
