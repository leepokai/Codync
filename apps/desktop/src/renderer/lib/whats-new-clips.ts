/** The What's new clips, bundled with the app: each feature's `media` name to its file's address. */
const files = import.meta.glob<string>('../../../../ios/Kit/Sources/CodyncUI/Resources/Announcements/*.mp4', { eager: true, query: '?url', import: 'default' })

const clips = new Map(Object.entries(files).map(([path, url]) => [path.slice(path.lastIndexOf('/') + 1, -'.mp4'.length), url]))

export const clipUrl = (media: string | undefined): string | undefined => (media ? clips.get(media) : undefined)

/** A bundled clip that doesn't load or decode should never happen: to Error tracking. */
export const clipFailed = () => window.codync.app.reportError({ process: 'desktop-renderer', type: 'ClipFailed', mechanism: 'whats-new-clip', frames: [] })
