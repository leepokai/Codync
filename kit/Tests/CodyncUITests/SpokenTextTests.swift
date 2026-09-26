import Testing
@testable import CodyncUI

@Test func spokenTextDropsMarkdown() {
    let reply = """
    ## Done

    Fixed **the login page** in `LoginView.swift`, see [the PR](https://x.y/1).

    ```swift
    let a = 1
    ```

    - kept `user_id` as is
    | File | Lines |
    |------|-------|
    | a.swift | 3 |
    """
    #expect(SpokenText.from(reply) == """
    Done
    Fixed the login page in LoginView.swift, see the PR.
    kept user_id as is
    File, Lines
    a.swift, 3
    """)
}
