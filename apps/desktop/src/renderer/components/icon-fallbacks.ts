import * as L from 'lucide'

type IconNode = L.IconNode

// Lucide has no brand marks (its Apple is the fruit): the filled logo, from Simple Icons (CC0).
const appleLogo: IconNode = [
  [
    'path',
    {
      fill: 'currentColor',
      stroke: 'none',
      d: 'M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701',
    },
  ],
]

// The codync-network mark (host/crates/network/assets/icon.png), drawn on every platform.
const codyncNetwork: IconNode = [['path', { fill: 'currentColor', stroke: 'none', d: 'M11.66 3.41c-0.05 0-0.2 0.03-0.33 0.06c-0.7 0.15-1.33 0.49-1.82 1c-0.6 0.62-0.91 1.34-0.95 2.17c-0.02 0.39 0.01 0.69 0.12 1.1c0.1 0.39 0.1 0.58-0 0.87c-0.14 0.39-0.28 0.57-0.93 1.17c-0.86 0.8-1.38 1.66-1.71 2.81c-0.12 0.41-0.24 0.66-0.43 0.87c-0.23 0.26-0.48 0.39-0.94 0.5c-0.26 0.06-0.61 0.2-0.87 0.33c-0.9 0.48-1.56 1.39-1.73 2.41c-0.05 0.31-0.04 0.92 0.02 1.21c0.14 0.64 0.46 1.22 0.94 1.69c0.55 0.55 1.19 0.85 1.97 0.93c0.67 0.07 1.38-0.1 1.99-0.46c0.28-0.17 0.4-0.27 0.82-0.7c0.42-0.42 0.51-0.49 0.81-0.54c0.27-0.05 0.56 0.02 1.07 0.26c0.86 0.4 1.38 0.54 2.14 0.57c0.79 0.03 1.56-0.13 2.39-0.52c0.19-0.09 0.45-0.2 0.57-0.24c0.38-0.13 0.72-0.1 0.99 0.07c0.11 0.06 0.25 0.21 0.62 0.61c0.38 0.42 0.77 0.68 1.28 0.85c0.34 0.12 0.51 0.14 1 0.14c0.41-0 0.47-0.01 0.72-0.07c0.66-0.17 1.13-0.43 1.59-0.91c0.49-0.49 0.8-1.05 0.94-1.74c0.07-0.35 0.07-0.94-0-1.3c-0.13-0.64-0.45-1.22-0.93-1.7c-0.61-0.62-1.38-0.95-2.37-1.04c-1.1-0.09-2.12 0.25-2.83 0.96c-0.22 0.21-0.38 0.43-0.87 1.1c-0.27 0.37-0.8 0.92-1.09 1.12c-0.69 0.48-1.41 0.68-2.19 0.62c-1.65-0.13-2.95-1.29-3.32-2.94c-0.07-0.32-0.08-1.14-0.01-1.47c0.24-1.16 0.95-2.18 1.96-2.77c0.34-0.2 0.79-0.36 1.2-0.44c0.3-0.05 0.95-0.04 1.25 0.03c0.83 0.18 1.51 0.61 1.87 1.2c0.11 0.17 0.23 0.45 0.38 0.89c0.1 0.29 0.2 0.46 0.36 0.63c0.5 0.54 1.31 0.56 1.81 0.04c0.16-0.16 0.29-0.4 0.33-0.58c0.04-0.16 0.03-0.48-0.01-0.67c-0.06-0.24-0.3-0.71-0.54-1.03c-0.12-0.16-0.41-0.48-0.69-0.77c-0.54-0.55-0.65-0.68-0.79-0.96c-0.18-0.37-0.21-0.65-0.12-1.03c0.25-1.04 0.12-1.97-0.4-2.77c-0.66-1.02-1.75-1.62-2.93-1.6c-0.15 0-0.31 0.01-0.36 0.01z' }]]

// The closest Lucide icon for each SF Symbol, for platforms without SF Symbols (Linux, Windows).
const map: Record<string, IconNode> = {
  archivebox: L.Archive,
  'arrow.clockwise': L.RotateCw,
  'arrow.counterclockwise': L.RotateCcw,
  'arrow.uturn.backward': L.Undo2,
  'arrow.down': L.ArrowDown,
  'arrow.down.to.line': L.Download,
  'arrow.down.circle': L.CircleArrowDown,
  'arrow.clockwise.circle': L.RotateCw,
  'arrow.down.right.and.arrow.up.left': L.Minimize2,
  'arrow.left': L.ArrowLeft,
  'arrow.left.arrow.right': L.ArrowLeftRight,
  'arrow.right': L.ArrowRight,
  'arrow.right.doc.on.clipboard': L.ClipboardCopy,
  'arrow.up.doc.on.clipboard': L.ClipboardPaste,
  'arrow.right.to.line': L.ArrowRightToLine,
  'arrow.triangle.2.circlepath': L.RefreshCw,
  'arrow.triangle.branch': L.GitBranch,
  'arrow.turn.down.right': L.CornerDownRight,
  'arrow.up': L.ArrowUp,
  'arrow.up.right.square': L.SquareArrowOutUpRight,
  'arrowshape.turn.up.left': L.Reply,
  'apple.logo': appleLogo,
  'bell.badge': L.BellDot,
  bolt: L.Zap,
  book: L.BookOpen,
  'book.closed': L.Book,
  'book.pages': L.BookOpenText,
  brain: L.Brain,
  'bubble.left': L.MessageSquare,
  'bubble.left.fill': L.MessageSquare,
  'bubble.left.and.bubble.right': L.MessagesSquare,
  calendar: L.Calendar,
  'chart.bar': L.ChartColumn,
  'chart.bar.doc.horizontal': L.FileChartColumn,
  checklist: L.ListChecks,
  checkmark: L.Check,
  'checkmark.circle': L.CircleCheck,
  'checkmark.circle.fill': L.CircleCheck,
  'checkmark.seal': L.BadgeCheck,
  'chevron.down': L.ChevronDown,
  'chevron.left': L.ChevronLeft,
  'chevron.left.2': L.ChevronsLeft,
  'chevron.right': L.ChevronRight,
  'chevron.right.2': L.ChevronsRight,
  'chevron.up': L.ChevronUp,
  'chevron.up.chevron.down': L.ChevronsUpDown,
  circle: L.Circle,
  'circle.dotted': L.CircleDashed,
  clock: L.Clock,
  'clock.arrow.circlepath': L.History,
  cloud: L.Cloud,
  'codync.network': codyncNetwork,
  cpu: L.Cpu,
  'cursorarrow.motionlines': L.MousePointerClick,
  desktopcomputer: L.Monitor,
  'desktopcomputer.trianglebadge.exclamationmark': L.MonitorX,
  display: L.Monitor,
  'display.2': L.MonitorSmartphone,
  'display.trianglebadge.exclamationmark': L.MonitorX,
  doc: L.File,
  'doc.badge.plus': L.FilePlus,
  'doc.on.clipboard': L.Clipboard,
  'doc.on.clipboard.fill': L.Clipboard,
  'doc.on.doc': L.Copy,
  'doc.plaintext': L.FileText,
  'doc.richtext': L.FileText,
  'doc.text': L.FileText,
  'doc.zipper': L.FileArchive,
  ellipsis: L.Ellipsis,
  'ellipsis.circle': L.CircleEllipsis,
  exclamationmark: L.TriangleAlert,
  'exclamationmark.circle': L.CircleAlert,
  'exclamationmark.circle.fill': L.CircleAlert,
  'exclamationmark.triangle': L.TriangleAlert,
  'exclamationmark.triangle.fill': L.TriangleAlert,
  eye: L.Eye,
  'eye.slash': L.EyeOff,
  film: L.Film,
  folder: L.Folder,
  'gauge.with.dots.needle.33percent': L.Gauge,
  gearshape: L.Settings,
  globe: L.Globe,
  'house.fill': L.House,
  hammer: L.Hammer,
  'hand.point.up.left': L.Pointer,
  'hand.raised': L.Hand,
  'hand.raised.slash': L.HandMetal,
  'info.circle': L.Info,
  iphone: L.Smartphone,
  key: L.Key,
  'key.fill': L.KeyRound,
  'keyboard.chevron.compact.down': L.Keyboard,
  laptopcomputer: L.Laptop,
  link: L.Link,
  'list.bullet': L.List,
  'list.bullet.rectangle': L.ScrollText,
  'lock.display': L.MonitorDot,
  'lock.shield': L.ShieldCheck,
  macmini: L.Server,
  'macpro.gen3': L.Server,
  macstudio: L.Server,
  macwindow: L.AppWindow,
  magnifyingglass: L.Search,
  mic: L.Mic,
  moon: L.Moon,
  'mic.fill': L.Mic,
  'mic.slash': L.MicOff,
  'mic.slash.fill': L.MicOff,
  'minus.circle': L.CircleMinus,
  'music.note': L.Music,
  network: L.Network,
  pencil: L.Pencil,
  person: L.User,
  'person.2': L.Users,
  'person.crop.circle': L.CircleUser,
  'person.crop.circle.badge.checkmark': L.UserCheck,
  'person.crop.circle.badge.plus': L.UserPlus,
  'person.fill': L.User,
  'phone.down.fill': L.PhoneOff,
  photo: L.Image,
  'photo.on.rectangle': L.Images,
  pin: L.Pin,
  'pin.fill': L.Pin,
  'pin.slash': L.PinOff,
  'play.circle': L.CirclePlay,
  plus: L.Plus,
  'plus.circle': L.CirclePlus,
  'point.3.connected.trianglepath.dotted': L.Waypoints,
  power: L.Power,
  qrcode: L.QrCode,
  'record.circle': L.CircleDot,
  'rectangle.and.hand.point.up.left': L.MousePointer2,
  'rectangle.portrait.and.arrow.right': L.LogOut,
  'rotate.right': L.RotateCw,
  safari: L.Compass,
  'server.rack': L.Server,
  shippingbox: L.Package,
  'sidebar.left': L.PanelLeft,
  'slider.horizontal.3': L.SlidersHorizontal,
  sparkles: L.Sparkles,
  'speaker.slash.fill': L.VolumeX,
  'speaker.wave.2': L.Volume2,
  'speaker.wave.2.fill': L.Volume2,
  'square.and.arrow.down': L.Import,
  'square.and.arrow.up': L.Share,
  'square.and.pencil': L.SquarePen,
  'square.grid.2x2': L.LayoutGrid,
  'square.grid.3x3': L.Grid3x3,
  'square.on.square': L.Copy,
  star: L.Star,
  'star.fill': L.Star,
  'stop.circle': L.CircleStop,
  'stop.fill': L.Square,
  terminal: L.Terminal,
  'text.bubble': L.MessageSquareText,
  tray: L.Inbox,
  'tray.and.arrow.up': L.ArchiveRestore,
  trash: L.Trash2,
  waveform: L.AudioLines,
  wifi: L.Wifi,
  'wifi.slash': L.WifiOff,
  'wrench.and.screwdriver': L.Wrench,
  xmark: L.X,
  'xmark.circle': L.CircleX,
  'xmark.circle.fill': L.CircleX,
}

const cache = new Map<string, string>()

export function fallbackSvg(name: string): string {
  const hit = cache.get(name)
  if (hit !== undefined) return hit
  const node = map[name] ?? L.Circle
  const svg = node
    .map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).filter(([, v]) => v !== undefined).map(([k, v]) => `${k}="${v}"`).join(' ')}/>`)
    .join('')
  cache.set(name, svg)
  return svg
}

/** The same icon as a canvas path in Lucide's 24-point box, for images drawn outside the DOM (menu icons). */
export function fallbackPath(name: string): Path2D {
  const path = new Path2D()
  const n = (v: unknown) => Number(v ?? 0)
  for (const [tag, a] of map[name] ?? L.Circle) {
    if (tag === 'path') path.addPath(new Path2D(String(a.d)))
    else if (tag === 'rect') path.roundRect(n(a.x), n(a.y), n(a.width), n(a.height), n(a.rx))
    else if (tag === 'line') {
      path.moveTo(n(a.x1), n(a.y1))
      path.lineTo(n(a.x2), n(a.y2))
    } else if (tag === 'circle' || tag === 'ellipse') {
      const [rx, ry] = tag === 'circle' ? [n(a.r), n(a.r)] : [n(a.rx), n(a.ry)]
      path.moveTo(n(a.cx) + rx, n(a.cy))
      path.ellipse(n(a.cx), n(a.cy), rx, ry, 0, 0, Math.PI * 2)
    } else {
      const points = String(a.points).trim().split(/[\s,]+/).map(Number)
      for (let i = 0; i + 1 < points.length; i += 2) path[i ? 'lineTo' : 'moveTo'](points[i], points[i + 1])
      if (tag === 'polygon') path.closePath()
    }
  }
  return path
}
