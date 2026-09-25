import CoreMedia
import Foundation
import ImageIO
@preconcurrency import ScreenCaptureKit
import UniformTypeIdentifiers
@preconcurrency import WebRTC

/// Hands ScreenCaptureKit frames to WebRTC. Lives on its own queue: frames never touch the main thread.
final class FrameSink: NSObject, SCStreamOutput, @unchecked Sendable {
    // @unchecked: every field is only touched on `queue` (SCStream's sample queue and the idle
    // timer both run there), and WebRTC's ObjC types carry no Sendable annotations.
    let queue = DispatchQueue(label: "com.pokai.Codync.screen.frames", qos: .userInteractive)
    private let source: RTCVideoSource
    private let capturer: RTCVideoCapturer
    private var last: RTCVideoFrame?
    private var lastAt: UInt64 = 0
    private var timer: DispatchSourceTimer?

    init(source: RTCVideoSource) {
        self.source = source
        capturer = RTCVideoCapturer(delegate: source)
        super.init()
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + 1, repeating: .milliseconds(500))
        t.setEventHandler { [weak self] in self?.repeatIfIdle() }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              let info = (CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]])?.first,
              let raw = info[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
              let pixels = CMSampleBufferGetImageBuffer(sample)
        else { return }
        deliver(RTCCVPixelBuffer(pixelBuffer: pixels))
    }

    /// ScreenCaptureKit only sends frames when something changes. A still screen still needs
    /// frames now and then so the encoder can answer keyframe requests after packet loss.
    private func repeatIfIdle() {
        guard let last, DispatchTime.now().uptimeNanoseconds - lastAt > 900_000_000 else { return }
        deliver(last.buffer)
    }

    private func deliver(_ buffer: RTCVideoFrameBuffer) {
        let now = DispatchTime.now().uptimeNanoseconds
        let frame = RTCVideoFrame(buffer: buffer, rotation: ._0, timeStampNs: Int64(now))
        last = frame
        lastAt = now
        source.capturer(capturer, didCapture: frame)
    }
}

/// Streams one display (or a region of it) into WebRTC at the size the phone can show.
@MainActor
final class Capture {
    let sink: FrameSink
    private var stream: SCStream?
    private(set) var display: CGDirectDisplayID
    /// Region to stream, in display points; `nil` = the whole display.
    private var crop: CGRect?
    /// Largest frame the viewer can use, in pixels (long edge, short edge).
    private var limit = (long: 2560.0, short: 1600.0)

    init(display: CGDirectDisplayID, source: RTCVideoSource) {
        self.display = display
        sink = FrameSink(source: source)
    }

    func start() async throws {
        let s = SCStream(filter: try await Self.filter(display), configuration: configuration(), delegate: nil)
        try s.addStreamOutput(sink, type: .screen, sampleHandlerQueue: sink.queue)
        try await s.startCapture()
        stream = s
    }

    func stop() async {
        sink.stop()
        try? await stream?.stopCapture()
        stream = nil
    }

    /// What the phone wants to see: display, region and its viewport size.
    /// Returns what's streamed from now on.
    @discardableResult
    func view(display: CGDirectDisplayID?, crop: CGRect?, long: Double?, short: Double?) async throws -> (display: CGDirectDisplayID, crop: CGRect?) {
        if let long, let short, long > 0, short > 0 {
            limit = (min(long, 3840), min(short, 2160))
        }
        self.crop = crop.map { $0.intersection(CGRect(origin: .zero, size: CGDisplayBounds(display ?? self.display).size)) }
            .flatMap { $0.width >= 16 && $0.height >= 16 ? $0 : nil }
        if let display, display != self.display {
            self.display = display
            try await stream?.updateContentFilter(try await Self.filter(display))
        }
        try await stream?.updateConfiguration(configuration())
        return (self.display, self.crop)
    }

    private func configuration() -> SCStreamConfiguration {
        let c = SCStreamConfiguration()
        let region = crop ?? CGRect(origin: .zero, size: CGDisplayBounds(display).size)
        let scale = Self.backingScale(display)
        let w = region.width * scale, h = region.height * scale
        let fit = min(1, limit.long / max(w, h), limit.short / min(w, h))
        // 4:2:0 needs even sizes.
        c.width = max(2, Int(w * fit / 2) * 2)
        c.height = max(2, Int(h * fit / 2) * 2)
        if crop != nil { c.sourceRect = region }
        c.minimumFrameInterval = CMTime(value: 1, timescale: 60)
        c.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        c.colorMatrix = CGDisplayStream.yCbCrMatrix_ITU_R_709_2
        c.showsCursor = true
        c.queueDepth = 6
        return c
    }

    static func backingScale(_ display: CGDirectDisplayID) -> Double {
        let points = CGDisplayBounds(display).width
        guard points > 0, let mode = CGDisplayCopyDisplayMode(display) else { return 2 }
        return Double(mode.pixelWidth) / points
    }

    /// The display without our own windows (the "viewing" badge).
    static func filter(_ display: CGDirectDisplayID) async throws -> SCContentFilter {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let d = content.displays.first(where: { $0.displayID == display }) else {
            throw HelperError("That display isn't connected anymore.")
        }
        let own = content.applications.filter { $0.processID == getpid() }
        return SCContentFilter(display: d, excludingApplications: own, exceptingWindows: [])
    }

    /// A still for agents: exactly `width`×`height`, JPEG, base64.
    static func screenshot(display: CGDirectDisplayID, width: Int, height: Int) async throws -> String {
        let cfg = SCStreamConfiguration()
        cfg.width = width
        cfg.height = height
        cfg.showsCursor = true
        let image = try await SCScreenshotManager.captureImage(contentFilter: try await filter(display), configuration: cfg)
        let data = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil) else {
            throw HelperError("Couldn't encode the screenshot.")
        }
        CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { throw HelperError("Couldn't encode the screenshot.") }
        return (data as Data).base64EncodedString()
    }
}
