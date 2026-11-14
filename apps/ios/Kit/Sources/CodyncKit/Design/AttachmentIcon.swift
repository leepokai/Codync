import Foundation

/// The symbol for a file by its extension.
public enum AttachmentIcon {
    public static func symbol(_ name: String) -> String {
        if Attachment.isImage(name) { return "photo" }
        if Attachment.isAudio(name) { return "waveform" }
        return switch (name as NSString).pathExtension.lowercased() {
        case "pdf": "doc.richtext"
        case "zip", "gz", "tar", "7z": "doc.zipper"
        case "mov", "mp4", "m4v": "film"
        default: "doc"
        }
    }
}
