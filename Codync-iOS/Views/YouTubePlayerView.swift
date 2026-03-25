import SwiftUI
import WebKit

struct YouTubePlayerView: UIViewRepresentable {
    let videoId: String

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        config.mediaTypesRequiringUserActionForPlayback = []
        let webView = WKWebView(frame: .zero, configuration: config)
        webView.isOpaque = false
        webView.backgroundColor = .black
        webView.scrollView.backgroundColor = .black
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        let url = URL(string: "https://www.youtube.com/watch?v=\(videoId)")!
        webView.load(URLRequest(url: url))
    }
}

private struct DemoVideoItem: Identifiable, Hashable {
    let id: String  // YouTube video ID
    let title: String
    let subtitle: String
    let icon: String
}

private let demoVideos: [DemoVideoItem] = [
    DemoVideoItem(id: "A5ki29svIc4", title: "App Overview", subtitle: "Full walkthrough on Mac & iPhone", icon: "play.rectangle"),
    DemoVideoItem(id: "9klpDsbHwJU", title: "Real Device Demo", subtitle: "iCloud sync between Mac, iPhone & Watch", icon: "iphone.and.arrow.forward"),
    DemoVideoItem(id: "N-qhrJugoZo", title: "Apple Watch", subtitle: "Live session monitoring on your wrist", icon: "applewatch"),
]

struct DemoVideoSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var selectedVideo: DemoVideoItem?

    var body: some View {
        NavigationStack {
            List(demoVideos) { video in
                Button {
                    selectedVideo = video
                } label: {
                    HStack(spacing: 14) {
                        Image(systemName: video.icon)
                            .font(.system(size: 20))
                            .foregroundStyle(.white)
                            .frame(width: 36, height: 36)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(video.title)
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundStyle(.white)
                            Text(video.subtitle)
                                .font(.system(size: 13))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Image(systemName: "play.circle.fill")
                            .font(.system(size: 22))
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }
            .listStyle(.plain)
            .navigationTitle("Demos")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .navigationDestination(item: $selectedVideo) { video in
                YouTubePlayerView(videoId: video.id)
                    .ignoresSafeArea(edges: .bottom)
                    .navigationTitle(video.title)
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .topBarTrailing) {
                            Button {
                                if let url = URL(string: "https://youtu.be/\(video.id)") {
                                    UIApplication.shared.open(url)
                                }
                            } label: {
                                HStack(spacing: 4) {
                                    Image(systemName: "arrow.up.forward")
                                        .font(.system(size: 11, weight: .semibold))
                                    Text("YouTube")
                                        .font(.system(size: 14, weight: .medium))
                                }
                            }
                        }
                    }
            }
        }
        .preferredColorScheme(.dark)
    }
}
