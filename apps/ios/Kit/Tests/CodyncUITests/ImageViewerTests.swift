import CoreGraphics
import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers
@testable import CodyncKit
@testable import CodyncUI

private let box = CGSize(width: 1000, height: 800)
private let small = CGSize(width: 600, height: 400)
private let big = CGSize(width: 2000, height: 1600)

@Test func aPictureSmallerThanTheBoxStaysCenteredAtFitAndWhileItStillFits() {
    #expect(ImageZoom(scale: 1, offset: CGSize(width: 50, height: -20)).clamped(box: box, picture: small) == .fit)
    // 600 wide at 1.5x is 900: still inside 1000, so no pan.
    #expect(ImageZoom(scale: 1.5, offset: CGSize(width: 40, height: 0)).clamped(box: box, picture: small)
        == ImageZoom(scale: 1.5, offset: .zero))
}

@Test func zoomStaysBetweenFitAndEightTimes() {
    #expect(ImageZoom.fit.zoomed(by: 0.2, at: .zero, box: box, picture: small).scale == 1)
    #expect(ImageZoom.fit.zoomed(by: 100, at: .zero, box: box, picture: small).scale == 8)
}

@Test func pinchingKeepsTheSpotUnderTheFingersWhereItWas() {
    let point = CGPoint(x: 300, y: -100)
    let start = ImageZoom(scale: 2, offset: CGSize(width: 100, height: 50))
    let after = start.zoomed(by: 1.5, at: point, box: box, picture: big)
    // The picture's own coordinate under the fingers, before and after.
    let under = { (z: ImageZoom) in CGPoint(x: (point.x - z.offset.width) / z.scale, y: (point.y - z.offset.height) / z.scale) }
    #expect(abs(under(after).x - under(start).x) < 1e-9)
    #expect(abs(under(after).y - under(start).y) < 1e-9)
}

@Test func draggingStopsAtThePictureEdge() {
    let zoomed = ImageZoom(scale: 2, offset: .zero)
    // 2000 * 2 = 4000 wide in a 1000 box: at most 1500 either way; 1600 * 2 in 800: 1200.
    #expect(zoomed.panned(by: CGSize(width: 5000, height: -5000), box: box, picture: big).offset == CGSize(width: 1500, height: -1200))
    #expect(zoomed.panned(by: CGSize(width: -5000, height: 30), box: box, picture: big).offset == CGSize(width: -1500, height: 30))
    #expect(ImageZoom.fit.panned(by: CGSize(width: 40, height: 40), box: box, picture: small) == .fit)
}

@Test func doubleTapZoomsInOnTheSpotThenBackToFit() {
    let zoomed = ImageZoom.fit.toggled(at: CGPoint(x: 100, y: 0), box: box, picture: big)
    #expect(zoomed.scale == 2.5)
    #expect(zoomed.offset == CGSize(width: -150, height: 0))
    #expect(zoomed.toggled(at: .zero, box: box, picture: big) == .fit)
}

@Test func copyingAPictureKeepsItsOwnBytesAndAddsAPNGOnlyForTypesFewAppsPaste() throws {
    let png = try encoded(.png)
    #expect(ImageClipboard.item(png)?.keys.sorted() == [UTType.png.identifier])
    #expect(ImageClipboard.item(png)?[UTType.png.identifier] == png)

    let tiff = try encoded(.tiff)
    let item = try #require(ImageClipboard.item(tiff))
    #expect(item[UTType.tiff.identifier] == tiff)
    let converted = try #require(item[UTType.png.identifier])
    #expect(ImageData.isPNG(converted))

    #expect(ImageClipboard.item(Data("not a picture".utf8)) == nil)
}

@Test func theClipboardPNGOfAHugePictureIsScaledDownToTheViewersSize() throws {
    let tiff = try encoded(.tiff, width: ImageData.largestPixels * 2, height: 4)
    let png = try #require(ImageClipboard.item(tiff)?[UTType.png.identifier])
    #expect(try pixelSize(png) == CGSize(width: ImageData.largestPixels, height: 2))
    #expect(ImageClipboard.item(tiff)?[UTType.tiff.identifier] == tiff)
}

@Test func theClipboardPNGIsUprightWhenThePictureIsStoredTurned() throws {
    // Stored 8 wide and 2 high, shown turned a quarter (EXIF orientation 6): upright it is 2 wide and 8 high.
    let tiff = try encoded(.tiff, width: 8, height: 2, orientation: .right)
    let png = try #require(ImageClipboard.item(tiff)?[UTType.png.identifier])
    #expect(try pixelSize(png) == CGSize(width: 2, height: 8))
}

@Test func aCopyOvertakenByANewerOneIsNoLongerTheOneToLand() {
    var requests = CopyRequests()
    let first = requests.start()
    #expect(requests.isLatest(first))
    let second = requests.start()
    #expect(!requests.isLatest(first))
    #expect(requests.isLatest(second))
}

@MainActor @Test func copyNameOvertakesAPictureCopyStillConverting() {
    let converting = ImageCopy.requests.start()
    ImageCopy.name(.sent(Attachment(id: "a", name: "chart.png", size: 1), botId: "b"), from: .menu)
    #expect(!ImageCopy.requests.isLatest(converting))
}

@MainActor @Test func aPictureTakesCopysPlaceInItsMessageMenuAndInTheViewerWhatLeavesClosesItFirst() {
    var log: [String] = []
    let entry = Entry(id: "e", seq: 1, botId: "b", rev: 1, kind: "agent", turn: 1, data: EntryData(text: "Hi"), createdAt: 0, updatedAt: 0)
    let menu = MessageMenu(entry: entry, reactions: ReactionPick(emoji: [], chosen: [], toggle: { _ in }), actions: [
        MenuItem("Reply") { log.append("reply") },
        MenuItem("Show what it did") { log.append("trace") },
    ])
    let copy = [MenuItem("Copy image") {}, MenuItem("Copy name") {}]
    #expect(menu.items(copy: copy).map(\.title) == ["Copy image", "Copy name", "Reply", "Show what it did"])
    #expect(menu.items(copy: menu.textCopy).map(\.title) == ["Copy", "Reply", "Show what it did"])

    let chosen = ReactionPick(emoji: ["👍"], chosen: ["👍"], toggle: { _ in })
    let inViewer = menu.inViewer(reactions: chosen) { log.append("close") }
    #expect(inViewer.reactions.chosen == ["👍"])
    inViewer.items(copy: copy)[2].action()
    #expect(log == ["close", "reply"])
}

@Test func theDownloadButtonSaysWhatItDoesNow() {
    #expect(FileDownloadCard.action(nil) == "Download")
    #expect(FileDownloadCard.action(.downloading(10)) == "Cancel download")
    #expect(FileDownloadCard.action(.failed("No network.")) == "Retry download")
    #expect(FileDownloadCard.action(.saved) == "Save again")
    #expect(FileDownloadCard.symbol(.downloading(10)) == "xmark")
    #expect(FileDownloadCard.symbol(.saved) == "arrow.down.to.line")
}

/// A `width` × `height` picture of `type`, stored with `orientation`.
private func encoded(_ type: UTType, width: Int = 2, height: Int = 2, orientation: CGImagePropertyOrientation = .up) throws -> Data {
    let context = try #require(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                         space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    context.setFillColor(CGColor(red: 1, green: 0, blue: 0, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let image = try #require(context.makeImage())
    let out = NSMutableData()
    let destination = try #require(CGImageDestinationCreateWithData(out, type.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, image, [kCGImagePropertyOrientation: orientation.rawValue] as CFDictionary)
    #expect(CGImageDestinationFinalize(destination))
    return out as Data
}

/// The pixel size `data` decodes to, as stored.
private func pixelSize(_ data: Data) throws -> CGSize {
    let source = try #require(CGImageSourceCreateWithData(data as CFData, nil))
    let properties = try #require(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
    let width = try #require(properties[kCGImagePropertyPixelWidth] as? Int)
    let height = try #require(properties[kCGImagePropertyPixelHeight] as? Int)
    return CGSize(width: width, height: height)
}
