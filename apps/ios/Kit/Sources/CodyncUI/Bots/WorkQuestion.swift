import CodyncKit
import SwiftUI

/// What Stop or Cut in asks about before it acts: a task of `bot`'s, on `store`'s computer.
struct WorkQuestion: Identifiable {
    enum Kind { case stop, cutIn }

    let kind: Kind
    let task: WorkTask
    let bot: Bot
    let store: BotStore
    var id: String { task.id }
}

extension BotStore {
    /// Does `action` on `task` of `bot`'s; Stop and Cut in ask first (`ask`, answered by `workQuestion`).
    func run(_ action: WorkAction, on task: WorkTask, of bot: Bot, ask: (WorkQuestion) -> Void) {
        switch action {
        case .stop: ask(WorkQuestion(kind: .stop, task: task, bot: bot, store: self))
        case .cutIn: ask(WorkQuestion(kind: .cutIn, task: task, bot: bot, store: self))
        case .first: workFirst(bot.id, task: task.id)
        case .drop: dropWork(bot.id, task: task.id)
        case .retry: retryWork(bot.id, task: task.id)
        }
    }
}

extension View {
    /// Stop's and Cut in's dialogs for `question`.
    func workQuestion(_ question: Binding<WorkQuestion?>) -> some View {
        let asked = question.wrappedValue
        return stopWorkDialog(isPresented: shown(question, .stop)) {
            if let asked { asked.store.stopWork(asked.bot.id, task: asked.task.id) }
        }
        .codyncDialog(L10n.fill(L10n.tr("Let \"{title}\" cut in"), ["title": asked?.task.title ?? ""]), isPresented: shown(question, .cutIn),
                      message: "Every thread is busy. Pick the task that gives way: its thread closes and the task starts again later.") {
            (asked.map { WorkSections($0.bot.work).givingWay } ?? []).map { giving in
                DialogAction(cutInChoice(giving)) {
                    if let asked { asked.store.workCutIn(asked.bot.id, task: asked.task.id, insteadOf: giving.id) }
                }
            }
        }
    }
}

private func shown(_ question: Binding<WorkQuestion?>, _ kind: WorkQuestion.Kind) -> Binding<Bool> {
    Binding(get: { question.wrappedValue?.kind == kind }, set: { if !$0 { question.wrappedValue = nil } })
}

/// A working task in the cut-in dialog: its title, what it does, how long.
private func cutInChoice(_ task: WorkTask) -> String {
    let elapsed = task.startedAt.map { Duration.seconds(max(0, Date.now.timeIntervalSince(Date(milliseconds: $0)))).formatted(.time(pattern: .minuteSecond)) }
    return [task.title, task.status == .needsYou || !task.activity.isEmpty ? task.what() : nil, elapsed]
        .compactMap(\.self).joined(separator: " · ")
}
