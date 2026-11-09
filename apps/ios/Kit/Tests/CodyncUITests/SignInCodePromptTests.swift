import Foundation
import Testing
@testable import CodyncUI

/// Starts `ask` and waits until the code is on screen.
@MainActor private func asking(_ prompt: SignInCodePrompt, _ code: String) async -> Task<Bool, Never> {
    let answer = Task { await prompt.ask(code) }
    while prompt.code == nil { await Task.yield() }
    return answer
}

@MainActor @Test func copyAndOpenAnswersYesWhicheverCloseComesFirst() async {
    let prompt = SignInCodePrompt()
    // The fading card: the action runs, then the dialog closes.
    var answer = await asking(prompt, "ABCD-1234")
    prompt.open()
    prompt.dismissed()
    #expect(await answer.value)
    #expect(prompt.code == nil)
    // A card that closes in place: the dialog closes, then the action runs.
    answer = await asking(prompt, "EFGH-5678")
    prompt.dismissed()
    prompt.open()
    #expect(await answer.value)
}

@MainActor @Test func cancellingTheCodeEndsTheSignInQuietly() async {
    let prompt = SignInCodePrompt()
    let answer = await asking(prompt, "ABCD-1234")
    prompt.dismissed()
    #expect(await answer.value == false)
    #expect(prompt.code == nil)
}
