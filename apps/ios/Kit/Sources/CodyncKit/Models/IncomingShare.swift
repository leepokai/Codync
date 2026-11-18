import Foundation
import UniformTypeIdentifiers

/// How the share extension reads what another app shared (`apps/ios/Share/SharedItems.swift` loads it): text and links
/// become the message, everything else a file.
public enum IncomingShare {
    /// A web link: a URL that isn't a file's.
    public static func isLink(_ types: [UTType]) -> Bool {
        types.contains { $0.conforms(to: .url) } && !types.contains { $0.conforms(to: .fileURL) }
    }

    /// Text that isn't a file: a file's provider has a name, so a `.txt` from Files stays a file.
    public static func isText(_ types: [UTType], suggestedName: String?) -> Bool {
        suggestedName == nil && types.contains { $0.conforms(to: .plainText) }
    }

    /// One item's lines, its `caption` (the words some apps put beside a link rather than in an attachment) first unless
    /// a line already says it.
    public static func lines(_ own: [String], caption: String?) -> [String] {
        guard let caption = caption?.trimmingCharacters(in: .whitespacesAndNewlines), !caption.isEmpty,
              !own.contains(caption) else { return own }
        return [caption] + own
    }

    /// The sender's name for a file, with `type`'s extension when it has none; `fallback` (the file's own) without one.
    public static func fileName(_ suggested: String?, type: UTType, fallback: String) -> String {
        guard let suggested, !suggested.isEmpty else { return fallback }
        guard (suggested as NSString).pathExtension.isEmpty, let ext = type.preferredFilenameExtension else { return suggested }
        return "\(suggested).\(ext)"
    }

    /// What goes as `pasted` with `message`: the `shared` text while the message still holds it, so memory never takes it
    /// for the user's own words; nothing once it's edited or deleted, so words the user took out never leave the phone.
    public static func pasted(_ shared: String, in message: String) -> [String] {
        shared.isEmpty || !message.contains(shared) ? [] : [shared]
    }
}
