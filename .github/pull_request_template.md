## Summary

<!-- What changes for the user, and why. On a release PR into main, this section becomes the GitHub release description. -->

## Validation

<!--
Required, and checked by CI (the "PR validation" check fails until this is filled in).
Keep every item and tick its box. When one doesn't apply, tick it anyway and add "N/A: <why>" after it.
A PR merges once CI passes and Greptile's confidence score on its latest commit is 4/5 or more.
For a feature, reaching that score is part of the PR.
-->

- [ ] Ran the change end to end on a real host plus every client it touches (not only builds and unit tests), headless where the tool allows
- [ ] Ran CI's checks for the parts touched (host: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; desktop: `npm run typecheck && npm test`; Android: the Gradle checks; iOS: the Kit tests)
- [ ] UI change: the same change lands in iOS, Android, desktop and the terminal UI (or says which client is unaffected and why), with screenshots
- [ ] iPhone or Android change: lands in both apps and was checked on the iOS Simulator and the Android emulator
- [ ] Platform code, packaging or the desktop main process: tested on macOS, Linux and Windows (say which ran)
- [ ] New feature: PostHog events added, docs (`docs/`, `web/content/docs/`) updated
- [ ] Made with Claude Code: ran the `after-change` workflow (`.claude/workflows/after-change.js`) from the change's worktree and pasted its final report below
- [ ] Targets `dev`, not `main`

### Notes

<!-- Optional: the after-change report, or anything else the reviewer should know. -->

## What's New: iPhone

<!--
Store notes, one section per phone app: iPhone goes to the App Store when this PR ships in an
iOS release, Android to Google Play when it ships in an Android release. One bullet per change
that app's users can see, in both languages. Leave a section's bullets empty when its users
can't see the change (host, desktop, CI, docs, or only the other phone app). A section for the
version in apps/ios/WhatsNew.md or apps/android/WhatsNew.md takes precedence; a release with
neither gets a generic "Bug fixes and improvements".
-->

### zh-Hant

-

### en-US

-

## What's New: Android

### zh-Hant

-

### en-US

-
