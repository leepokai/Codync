import Foundation
import Testing
@testable import CodyncUI

// The same cases as Android's, the desktop's and the terminal's autolink tests.
private func urls(_ text: String) -> [String] { Autolink.find(text).map(\.url.absoluteString) }

@Test func autolinkFindsBareAddressesAndLeavesTheSentenceAroundThem() {
    #expect(urls("see https://codync.dev.") == ["https://codync.dev"])
    #expect(urls("open www.example.com/a?b=1, then") == ["https://www.example.com/a?b=1"])
    #expect(urls("(at https://example.com/x)") == ["https://example.com/x"])
    #expect(urls("https://en.wikipedia.org/wiki/Rust_(language)") == ["https://en.wikipedia.org/wiki/Rust_(language)"])
    #expect(urls("網址：https://codync.dev，謝謝") == ["https://codync.dev"])
    #expect(urls("HTTP://A.COM and http://b.io/x_y_z") == ["HTTP://A.COM", "http://b.io/x_y_z"])
}

@Test func autolinkSkipsWhatOnlyLooksLikeAnAddress() {
    #expect(urls("main.rs README.md codync.dev").isEmpty)
    #expect(urls("xhttps://a.com nohttp").isEmpty)
    #expect(urls("https:// and www.").isEmpty)
}

/// A reply's bare address becomes a link; one in a code span or already a link stays as it was.
@Test func repliesLinkBareAddressesOutsideCodeAndLinks() {
    let text = MarkdownText.inline("see https://a.com, `https://b.com` and [docs](https://c.com/x)")
    let links = text.runs.compactMap { run in run.link.map { ($0.absoluteString, String(text[run.range].characters)) } }
    #expect(links.map(\.0) == ["https://a.com", "https://c.com/x"])
    #expect(links.map(\.1) == ["https://a.com", "docs"])
}
