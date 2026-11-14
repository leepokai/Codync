import Foundation

/// A file going out with the next message: picked in the composer, or shared from another app.
public struct OutgoingFile: Identifiable, Sendable {
    public let id = UUID()
    public let name: String
    public let data: Data

    public init(name: String, data: Data) {
        self.name = name
        self.data = data
    }

    /// Anything larger is refused by the computer.
    public static let maxSize = 100 * 1024 * 1024

    /// Images the agents can't all read (HEIC, TIFF, BMP…) become JPEG, upright.
    public static func prepared(name: String, data: Data) -> OutgoingFile {
        let ext = (name as NSString).pathExtension.lowercased()
        guard ["heic", "heif", "tif", "tiff", "bmp"].contains(ext) || (ext.isEmpty && ImageData.isImage(data)),
              let jpeg = ImageData.jpeg(data) else { return OutgoingFile(name: name, data: data) }
        let base = (name as NSString).deletingPathExtension
        return OutgoingFile(name: (base.isEmpty ? "image" : base) + ".jpg", data: jpeg)
    }

    /// A name for a file that came without one (pasted or dragged image data).
    public static func unnamed(_ ext: String) -> String {
        "pasted-\(Int(Date.now.timeIntervalSince1970)).\(ext)"
    }
}
