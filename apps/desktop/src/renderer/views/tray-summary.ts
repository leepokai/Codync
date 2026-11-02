import { useEffect, useState } from 'react'
import { TRAY_SYMBOLS, type TrayImage, type TraySummary } from '@shared/ipc'
import { isGroup, isWorking, needsInput, folderName, type Bot, type UsageWindow } from '@shared/models'
import type { NetworkView } from '@shared/network'
import { iconBitmap, drawIconBitmap } from '../lib/icon-bitmap'
import { drawCharacter, moodOf } from '../components/Avatar'
import { fallbackPath } from '../components/icon-fallbacks'
import { botsTemplate, presenceTitle, roleTitle } from '../lib/network-copy'
import { useModels } from '../lib/observable'
import { resetLine, resolvedLanguage, translate } from '../lib/l10n'
import { prefs, usePref } from '../lib/prefs'
import { avatarColor, isDark, palette } from '../lib/theme'
import type { AppModel } from '../store/app-model'
import { providerTint, resetDate, untilText, windowReading, windowTitle } from './usage/usage-model'
import { drawMascot, nearlyUsedUp } from './usage/ProviderMascot'

const isMac = window.codync.platform === 'darwin'

/** Rendered icons by what they show and the appearance: nothing is redrawn while nothing changed. */
const cache = new Map<string, TrayImage>()

/**
 * A menu image drawn for 1x and 2x screens (a 1x screen shrinking a 2x image blurs it), or null
 * while `draw` can't draw yet (it returns false).
 */
function canvasImage(key: string, width: number, height: number, draw: (ctx: CanvasRenderingContext2D) => boolean | void): TrayImage | null {
  const full = `${key}|${isDark()}`
  const hit = cache.get(full)
  if (hit) return hit
  const at = (ratio: number) => {
    const canvas = document.createElement('canvas')
    canvas.width = width * ratio
    canvas.height = height * ratio
    const ctx = canvas.getContext('2d')!
    ctx.scale(ratio, ratio)
    return draw(ctx) === false ? null : canvas.toDataURL('image/png')
  }
  const x1 = at(1)
  const x2 = x1 && at(2)
  if (!x1 || !x2) return null
  const image = { x1, x2 }
  if (cache.size >= 512) cache.delete(cache.keys().next().value!)
  cache.set(full, image)
  return image
}

function avatarIcon(bot: Bot) {
  const bitmap = iconBitmap(bot.customIcon)?.image
  const key = `bot|${bot.customIcon?.dataUrl ?? ""}|${bot.customIcon?.tinted}|${bitmap?.complete}|${bot.id}|${bot.avatarShape}|${bot.avatarColor}|${bot.status}|${bot.unread > 0}`
  return canvasImage(key, 20, 20, (ctx) => {
    const dark = isDark()
    if (!drawIconBitmap(ctx, bot.customIcon, 20, avatarColor(bot.avatarColor))) drawCharacter(ctx, { shape: bot.avatarShape, fill: avatarColor(bot.avatarColor), size: 20, mood: moodOf(bot, false), dark, t: 0, still: true })
    const badge = needsInput(bot) ? palette(dark).warning : bot.unread > 0 ? (dark ? '#FFFFFF' : '#000000') : null
    if (badge) {
      const r = needsInput(bot) ? 3.6 : 2.8
      ctx.beginPath()
      ctx.arc(20 - r, 20 - r, r + 1, 0, Math.PI * 2)
      ctx.fillStyle = dark ? '#1e1e1e' : '#f2f2f2'
      ctx.fill()
      ctx.beginPath()
      ctx.arc(20 - r, 20 - r, r, 0, Math.PI * 2)
      ctx.fillStyle = badge
      ctx.fill()
    }
  })
}

/** A bot's line under its name: that it needs you, what it's doing, or its last message. */
export function botDetail(bot: Bot) {
  return needsInput(bot) ? 'Needs your response' : isWorking(bot) ? bot.activity || 'Working…' : (bot.lastMessage ?? folderName(bot))
}

/** One usage limit's bar in the provider's color: amber from 70%, red from 90%. */
function barIcon(providerId: string, window: UsageWindow) {
  const percent = Math.round(window.percent)
  return canvasImage(`bar|${providerId}|${percent}`, 60, 16, (ctx) => {
    const dark = isDark()
    const color = window.percent >= 90 ? palette(dark).danger : window.percent >= 70 ? palette(dark).warning : providerTint(providerId)
    ctx.fillStyle = dark ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.12)'
    ctx.beginPath()
    ctx.roundRect(0, 5.5, 60, 5, 2.5)
    ctx.fill()
    if (window.percent > 0) {
      ctx.fillStyle = color
      ctx.beginPath()
      ctx.roundRect(0, 5.5, Math.max(5, 60 * Math.min(1, window.percent / 100)), 5, 2.5)
      ctx.fill()
    }
  })
}

/** A network computer's dot, as in Settings → Network: green while it answers. */
function dotIcon(online: boolean) {
  return canvasImage(`dot|${online}`, 16, 16, (ctx) => {
    ctx.fillStyle = online ? palette(isDark()).added : palette(isDark()).tertiary
    ctx.beginPath()
    ctx.arc(8, 8, 3.5, 0, Math.PI * 2)
    ctx.fill()
  })
}

/** The Network row's codync-network mark: black for a macOS template image, else the menu's text color. */
function networkIcon() {
  return canvasImage('network', 16, 16, (ctx) => {
    ctx.scale(16 / 24, 16 / 24)
    ctx.fillStyle = !isMac && isDark() ? '#FFFFFF' : '#000000'
    ctx.fill(fallbackPath('codync.network'))
  })
}

/** The actions' icons for Windows and Linux menus, from the same fallbacks the app's icons use there. */
function symbolIcons(): TraySummary['symbols'] {
  if (isMac) return null
  const icons: NonNullable<TraySummary['symbols']> = {}
  for (const name of TRAY_SYMBOLS) {
    const icon = canvasImage(`symbol|${name}`, 16, 16, (ctx) => {
      ctx.scale(16 / 24, 16 / 24)
      ctx.strokeStyle = isDark() ? '#FFFFFF' : '#000000'
      ctx.lineWidth = 2
      ctx.lineCap = 'round'
      ctx.lineJoin = 'round'
      ctx.stroke(fallbackPath(name))
    })
    if (icon) icons[name] = icon
  }
  return icons
}

/** The menu's language: the app's setting, else the system's. */
const systemLanguage = () => (typeof navigator === 'undefined' ? 'en' : navigator.language)
const tr = (text: string) => translate(text, prefs.appLanguage.get(), systemLanguage())

/** "home · Owner" and "desk · Online · 3 bots" per network; null on hosts without networks. */
function networkLines(view: NetworkView | null | undefined): TraySummary['networks'] {
  if (!view) return null
  return view.networks.map((net) => ({
    title: `${net.name} · ${tr(roleTitle(net.role))}`,
    computers: net.computers.map((c) => ({
      label: [c.name, tr(presenceTitle(c)), tr(botsTemplate(c)).replaceAll('{count}', String(c.agents.length))].join(' · '),
      dot: dotIcon(c.me || c.online),
    })),
  }))
}

function usageLine(w: UsageWindow) {
  const reset = resetDate(w)
  const spoken = reset ? resetLine(`resets in ${untilText(reset)}`, prefs.appLanguage.get(), systemLanguage()) : ''
  return `${tr(windowTitle(w))} · ${windowReading(w, tr)}${spoken ? ` · ${spoken}` : ''}`
}

/** Sends what the menu bar shows whenever the stores change. */
export function useTraySummary(app: AppModel) {
  const [, redraw] = useState(0)
  const stores = [...app.stores.values()]
  useModels(stores)
  const [textSize] = usePref(prefs.textSize)
  const [iconStyle] = usePref(prefs.usageIconStyle)
  const [language] = usePref(prefs.appLanguage)
  useEffect(() => {
    let current = true
    const pending = app.roster.map(({ bot }) => iconBitmap(bot.customIcon)).filter((entry) => entry && !entry.image.complete)
    if (pending.length) void Promise.all(pending.map((entry) => entry!.ready)).then(() => { if (current) redraw((n) => n + 1) })
    return () => { current = false }
  })
  useEffect(() => {
    const timer = setTimeout(() => {
      const local = app.local
      const roster = app.roster.filter((i) => !isGroup(i.bot))
      const status = app.approvals.length
        ? 'A device asks for access'
        : app.needsAttention
          ? 'A bot needs you'
          : app.working > 0
            ? `${app.working} bot${app.working === 1 ? '' : 's'} working`
            : 'Connected'
      const screen = local?.screen
      const summary: TraySummary = {
        status,
        canPair: !!local,
        approval: app.approvals[0]?.request.deviceName ?? null,
        bots: roster.map((i) => ({
          ref: `${i.ref.computerId}/${i.ref.botId}`,
          name: app.computers.length > 1 ? `${i.bot.name} · ${i.computer.name}` : i.bot.name,
          detail: botDetail(i.bot),
          working: isWorking(i.bot),
          avatar: avatarIcon(i.bot),
        })),
        usage: (local?.usage.providers ?? []).map((p) => ({
          name: p.name,
          icon: canvasImage(`mascot|${p.id}|${iconStyle}|${nearlyUsedUp(p)}`, 16, 16, (ctx) => drawMascot(ctx, p, 16, iconStyle, isDark())),
          lines: p.windows.map((w) => ({ title: usageLine(w), bar: w.text ? null : barIcon(p.id, w) })),
        })),
        screen: screen
          ? {
              enabled: screen.enabled,
              subtitle: screenSubtitle(app),
              error: app.screenError,
            }
          : null,
        networks: networkLines(local?.network),
        networkIcon: networkIcon(),
        symbols: symbolIcons(),
        needsAttention: app.needsAttention,
        textSize,
        usageIconStyle: iconStyle,
        language: resolvedLanguage(language, systemLanguage()),
      }
      window.codync.app.setTraySummary(summary)
    }, 150)
    return () => clearTimeout(timer)
  })
}

function screenSubtitle(app: AppModel) {
  const local = app.local
  const screen = local?.screen
  if (!local || !screen || !screen.enabled) return `Control this ${isMac ? 'Mac' : 'computer'} from your iPhone`
  if (app.screenNeedsApproval) return 'Needs approval in System Settings'
  if (!screen.connected) return 'Starting Codync Screen…'
  if (!screen.capture || !screen.input) return 'Needs permission'
  if (screen.viewers > 0) return screen.viewers === 1 ? 'Your phone is viewing' : `${screen.viewers} viewers`
  const bot = screen.agentBot ? local.bots.get(screen.agentBot) : null
  if (bot) return `${bot.name} is using it`
  const display = screen.displays.find((d) => d.main)?.name ?? screen.displays[0]?.name
  return display ? `Ready · ${display}` : 'Ready'
}
