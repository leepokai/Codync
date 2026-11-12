import { app, Menu } from 'electron'
import type { TraySummary, WindowCommand } from '../shared/ipc'
import { translate } from '../renderer/lib/l10n'

const isMac = process.platform === 'darwin'
let shown: TraySummary['language'] | null = null

/**
 * The application menu, a system menu like the tray's, in `language`; rebuilt only when it changes.
 * Each item keeps its role's action and shortcut under a translated label.
 */
export function applyMenu(language: TraySummary['language'], send: (command: WindowCommand, show?: boolean) => void) {
  if (language === shown) return
  shown = language
  const tr = (text: string) => translate(text, language, 'en')
  const name = app.getName()
  const template: Electron.MenuItemConstructorOptions[] = [
    ...(isMac ? [{
      label: name,
      submenu: [
        { role: 'about' as const, label: tr('About {name}').replaceAll('{name}', name) },
        { type: 'separator' as const },
        { role: 'services' as const, label: tr('Services') },
        { type: 'separator' as const },
        { role: 'hide' as const, label: tr('Hide {name}').replaceAll('{name}', name) },
        { role: 'hideOthers' as const, label: tr('Hide Others') },
        { role: 'unhide' as const, label: tr('Show All') },
        { type: 'separator' as const },
        { role: 'quit' as const, label: tr('Quit {name}').replaceAll('{name}', name) },
      ],
    }] : []),
    {
      label: tr('File'),
      submenu: [isMac ? { role: 'close', label: tr('Close') } : { role: 'quit', label: tr('Quit {name}').replaceAll('{name}', name) }],
    },
    {
      label: tr('Edit'),
      submenu: [
        { role: 'undo', label: tr('Undo') },
        { role: 'redo', label: tr('Redo') },
        { type: 'separator' },
        { role: 'cut', label: tr('Cut') },
        { role: 'copy', label: tr('Copy') },
        { role: 'paste', label: tr('Paste') },
        ...(isMac ? [
          { role: 'pasteAndMatchStyle' as const, label: tr('Paste and Match Style') },
          { role: 'delete' as const, label: tr('Delete') },
          { role: 'selectAll' as const, label: tr('Select All') },
          { type: 'separator' as const },
          {
            label: tr('Substitutions'),
            submenu: [
              { role: 'showSubstitutions' as const, label: tr('Show Substitutions') },
              { type: 'separator' as const },
              { role: 'toggleSmartQuotes' as const, label: tr('Smart Quotes') },
              { role: 'toggleSmartDashes' as const, label: tr('Smart Dashes') },
              { role: 'toggleTextReplacement' as const, label: tr('Text Replacement') },
            ],
          },
          {
            label: tr('Speech'),
            submenu: [
              { role: 'startSpeaking' as const, label: tr('Start Speaking') },
              { role: 'stopSpeaking' as const, label: tr('Stop Speaking') },
            ],
          },
        ] : [
          { role: 'delete' as const, label: tr('Delete') },
          { type: 'separator' as const },
          { role: 'selectAll' as const, label: tr('Select All') },
        ]),
      ],
    },
    {
      label: tr('View'),
      submenu: [
        { label: tr('Bigger'), accelerator: 'CmdOrCtrl+=', click: () => send({ kind: 'stepTextSize', up: true }, false) },
        { label: tr('Smaller'), accelerator: 'CmdOrCtrl+-', click: () => send({ kind: 'stepTextSize', up: false }, false) },
        { label: tr('Actual Size'), accelerator: 'CmdOrCtrl+0', click: () => send({ kind: 'stepTextSize', up: null }, false) },
        { type: 'separator' },
        { label: tr('New bot'), accelerator: 'CmdOrCtrl+N', click: () => send({ kind: 'newBot' }) },
        { label: tr('Search Bots'), accelerator: 'CmdOrCtrl+F', click: () => send({ kind: 'search' }) },
        { label: tr('Toggle Sidebar'), accelerator: 'Ctrl+CmdOrCtrl+S', click: () => send({ kind: 'toggleSidebar' }, false) },
        { label: tr('Conversation details'), accelerator: 'Alt+CmdOrCtrl+I', click: () => send({ kind: 'toggleDetails' }, false) },
        { label: tr('Refresh'), accelerator: 'CmdOrCtrl+R', click: () => send({ kind: 'refresh' }, false) },
        { type: 'separator' },
        // The dev page reload sits on ⇧⌘R, leaving ⌘R to Refresh.
        ...(app.isPackaged ? [] : [
          { role: 'toggleDevTools' as const, label: tr('Toggle Developer Tools') },
          { role: 'forceReload' as const, label: tr('Force Reload') },
        ]),
        { role: 'togglefullscreen', label: tr('Toggle Full Screen') },
      ],
    },
    {
      // The role marks this submenu as the window list on macOS. A custom submenu is kept.
      label: tr('Window'),
      role: 'windowMenu',
      submenu: [
        { role: 'minimize', label: tr('Minimize') },
        { role: 'zoom', label: tr('Zoom') },
        ...(isMac ? [
          { type: 'separator' as const },
          { role: 'front' as const, label: tr('Bring All to Front') },
        ] : [{ role: 'close' as const, label: tr('Close') }]),
      ],
    },
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

