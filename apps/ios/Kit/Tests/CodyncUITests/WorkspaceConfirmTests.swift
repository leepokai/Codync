import CodyncKit
import Foundation
import Testing
@testable import CodyncUI

private struct Unsupported: Error {}
private struct Removed: LocalizedError { var errorDescription: String? { "Folder was removed" } }

private func listing(_ path: String, recent: Bool) -> DirListing {
    let json = #"{"path":"\#(path)","isGit":false,"dirs":[]\#(recent ? #","recent":[]"# : "")}"#
    return try! JSONDecoder().decode(DirListing.self, from: Data(json.utf8))
}

@MainActor @Test func olderHostValidatesWithoutTheSelectionMethod() async {
    let confirm = WorkspaceConfirm()
    let chosen = await confirm.confirm(
        "/project", listing: listing("/project", recent: false),
        select: { _ in throw Unsupported() }, fetch: { listing($0 ?? "", recent: false) })
    #expect(chosen == "/project")
}

@MainActor @Test func failedSelectionStaysWithItsError() async {
    let confirm = WorkspaceConfirm()
    let chosen = await confirm.confirm(
        "/gone", listing: listing("/gone", recent: true),
        select: { _ in throw Removed() }, fetch: { listing($0 ?? "", recent: true) })
    #expect(chosen == nil)
    #expect(confirm.error == "Folder was removed")
    #expect(!confirm.saving)
}
