import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appLanguages, resolvedLanguage, resetLine, languageTitle, systemIsChinese, systemScript, translate, usesChinese } from './l10n.ts'

test('chinese follows the choice, and a system preference follows the computer', () => {
  assert.equal(usesChinese('zh', false), true)
  assert.equal(usesChinese('en', true), false)
  assert.equal(usesChinese('system', true), true)
  assert.equal(usesChinese('system', false), false)
  assert.equal(usesChinese('zh-Hant', false), true)
  assert.equal(systemIsChinese('zh-Hans-CN'), true)
  assert.equal(systemIsChinese('zh-Hant-TW'), true)
  assert.equal(systemIsChinese('en-US'), false)
  assert.equal(systemScript('zh-TW'), 'hant')
  assert.equal(systemScript('zh-HK'), 'hant')
  assert.equal(systemScript('zh-Hant-TW'), 'hant')
  assert.equal(systemScript('zh_MO.UTF-8'), 'hant')
  assert.equal(systemScript('zh-CN'), 'hans')
  assert.equal(systemScript('zh-Hans-CN'), 'hans')
  assert.equal(systemScript('en-US'), null)
})

test('known chrome becomes Chinese and everything else stays', () => {
  assert.equal(translate('Pair a computer', 'zh', false), '配对一台电脑')
  assert.equal(translate('  Pair a computer\n', 'zh', false), '配对一台电脑')
  assert.equal(translate('Pair a computer', 'en', true), 'Pair a computer')
  assert.equal(translate('Not a phrase', 'system', true), 'Not a phrase')
  assert.equal(languageTitle('system', 'zh', false), '跟随系统')
  assert.equal(languageTitle('zh', 'en', true), '简体中文')
  assert.equal(languageTitle('zh-Hant', 'en', true), '繁體中文')
  assert.equal(translate('Settings', 'zh-Hant', 'en'), '設定')
  assert.equal(translate('Files', 'zh-Hant', 'en'), '檔案')
  assert.equal(translate('Settings', 'system', 'zh-TW'), '設定')
  assert.equal(translate('Settings', 'system', 'zh-CN'), '设置')
  assert.equal(languageTitle('en', 'zh', false), 'English')
  assert.equal(translate('Personal', 'zh', false), '个人')
  assert.equal(translate('This Mac', 'zh', false), '这台 Mac')
  assert.equal(translate('Show in notch', 'zh', false), '在屏幕顶部显示')
  assert.equal(translate('Show in Notch', 'zh', false), '在屏幕顶部显示')
  assert.equal(translate('Voice input shortcut', 'zh', false), '语音输入快捷键')
  assert.equal(translate('Right ⌘ twice', 'zh', false), '右 ⌘ 两次')
  assert.equal(translate('Left ⌘ twice', 'zh', false), '左 ⌘ 两次')
  assert.equal(translate('Claude Code (not installed)', 'zh', false), 'Claude Code（未安装）')
  assert.equal(translate('Copy template', 'zh', false), '复制模板')
  assert.equal(translate('Siri and Dictation are disabled', 'zh', false), 'Siri 和听写已关闭')
  assert.equal(
    translate('the Codync cloud refused (403 forbidden): Sign in on the computer to use remote screen relay.', 'zh', false),
    'Codync 云拒绝了（403 禁止）：请先在电脑上登录，才能使用远程屏幕中继。',
  )
  assert.equal(
    translate('Tip: double-tap the right ⌘ to talk from any app, then tap it once to send.', 'zh', false),
    '提示：连按两次右侧 ⌘，就能在任何应用里说话，再按一次就发送。',
  )
  assert.equal(translate('Theme', 'zh', false), '主题')
  assert.equal(translate('Only changes appearance on this device.', 'zh', false), '只改变这台设备的外观。')
  assert.equal(translate('Continue on this Mac', 'zh', false), '在这台 Mac 上继续')
  assert.equal(translate('Your coding agents,\nas teammates.', 'zh', false), '你的编程助手，\n像队友一样。')
  assert.equal(translate('Open {name}', 'zh', false).replaceAll('{name}', 'Codync Dev'), '打开 Codync Dev')
  assert.equal(translate('Quit {name}', 'zh', false).replaceAll('{name}', 'Codync Dev'), '退出 Codync Dev')
  assert.equal(translate('Pair iPhone…', 'zh', false), '配对 iPhone…')
  assert.equal(translate('Remote screen', 'zh', false), '远程屏幕')
  assert.equal(translate('Text Size', 'zh', false), '文字大小')
  assert.equal(translate('Restart host', 'zh', false), '重启 Host')
  assert.equal(translate('2 bots working', 'zh', false), '2 个 Bot 正在工作')
  assert.equal(translate('Check for updates', 'zh', false), '检查更新')
  assert.equal(translate('Computer access is ready.', 'zh', false), '电脑访问已就绪。')
  assert.equal(translate('Storage', 'zh', false), '存储')
  assert.equal(translate('Extensions sections', 'zh', false), '扩展分类')
  assert.equal(
    translate('listing skills on GitHub: https://api.github.com/repos/anthropics/skills answered 403 Forbidden', 'zh', false),
    '无法列出 GitHub 上的技能：https://api.github.com/repos/anthropics/skills 返回了 403 Forbidden',
  )
  assert.equal(
    translate('Remote screen is off. On the computer, open Codync → Settings → Computer access. Permissions are set separately on each computer.', 'zh', false),
    '远程屏幕已关闭。在电脑上打开 Codync → 设置 → 电脑访问。每台电脑的权限要单独设置。',
  )
  assert.equal(translate('Updates are available in release builds.', 'zh', false), '正式版才会提供更新。')
  assert.equal(translate('Privacy', 'zh', false), '隐私')
  assert.equal(translate('Share usage analytics', 'zh', false), '共享用量分析')
  assert.equal(translate('Fast mode', 'zh', false), '快速模式')
  assert.equal(translate('Use the computer', 'zh', false), '使用电脑')
  assert.equal(translate('{size} pt (Default)', 'zh', false), '{size} pt（默认）')
  assert.equal(translate('Full conversation', 'zh', false), '完整对话')
  assert.equal(translate('Settings', 'zh', false), '设置')
  assert.equal(translate('Ask {name}', 'zh', false), '问 {name}')
  assert.equal(translate('Stopped.', 'zh', false), '已停止。')
  assert.equal(translate('Load earlier messages', 'zh', false), '加载更早的消息')
  assert.equal(translate('Today', 'zh', false), '今天')
  assert.equal(translate('Yesterday', 'zh', false), '昨天')
  assert.equal(translate('No dreams yet.', 'zh', false), '还没有梦境。')
  assert.equal(translate('Standing instructions', 'zh', false), '常驻指令')
  assert.equal(translate('Your coding agents, as teammates.', 'zh', false), '你的编程智能体，像队友一样。')
  assert.equal(translate('Mark as Read', 'zh', false), '标为已读')
  assert.equal(translate('Resuming session…', 'zh', false), '正在恢复会话…')
  assert.equal(translate('Search agents, connectors and skills', 'zh', false), '搜索智能体、连接器和技能')
  assert.equal(translate('Apps', 'zh', false), '应用')
  assert.equal(translate('Agents', 'zh', false), '智能体')
  assert.equal(translate('Installed', 'zh', false), '已安装')
  assert.equal(translate('Not installed', 'zh', false), '未安装')
  assert.equal(translate('Set up', 'zh', false), '设置')
  assert.equal(translate('Writing a reply…', 'zh', false), '正在写回复…')
  assert.equal(translate('Downloading Codebuddy Code…', 'zh', false), '正在下载 Codebuddy Code…')
  assert.equal(translate('Needs approval: Bash', 'zh', false), '需要批准：Bash')
  assert.equal(translate('Gmail, Slack, GitHub, Notion and 1,000 more apps', 'zh', false), 'Gmail、Slack、GitHub、Notion 等 1,000 多个应用')
  assert.equal(translate('Storage', 'zh', false), '存储')
  assert.equal(translate('New skill', 'zh', false), '新建技能')
  assert.equal(translate('Composio API key', 'zh', false), 'Composio API 密钥')
  assert.equal(translate('When to use it', 'zh', false), '何时使用')
  assert.equal(translate("Runs on {name} in the bot's project folder. Quotes work like in a shell.", 'zh', false), '在 {name} 上、Bot 的项目文件夹里运行。引号用法和 shell 一样。')
  assert.equal(translate('Add connector', 'zh', false), '添加连接器')
  assert.equal(translate('Name your group', 'zh', false), '给群聊起个名字')
  assert.equal(translate('Nothing installed', 'zh', false), '还没有安装')
  assert.equal(translate('Remove {name}?', 'zh', false), '移除 {name}？')
  assert.equal(translate('No setup needed.', 'zh', false), '无需设置。')
  assert.equal(translate('On {name} ({kind})', 'zh', false), '在 {name} 上（{kind}）')
  assert.equal(translate('Runs the official installer on this Mac.', 'zh', false), '在 这台 Mac 上运行官方安装程序。')
  assert.equal(translate('Use Claude subscription', 'zh', false), '使用 Claude 订阅')
  assert.equal(translate('Support', 'zh', false), '支持')
  assert.equal(translate('Devices that can use FORWARDXU-MC3', 'zh', false), '可以使用 FORWARDXU-MC3 的设备')
  assert.equal(translate('Paired with a QR code · seen 10:54', 'zh', false), '通过二维码配对 · 10:54 见过')
  assert.equal(translate("CodeBuddy Code's own memory", 'zh', false), 'CodeBuddy Code 自己的记忆')
  assert.equal(translate("About {name}'s own memory", 'zh', false), '关于 {name} 自己的记忆')
  assert.equal(translate('A memory of its own', 'zh', false), '它自己的记忆')
  assert.equal(translate('Usage', 'zh', false), '用量')
  assert.equal(translate('Computers & devices', 'zh', false), '电脑和设备')
  assert.equal(translate('Reachable from anywhere through Cloudflare (encrypted).', 'zh', false), '通过 Cloudflare 随时随地连接（已加密）。')
  assert.equal(
    translate('Reachable from anywhere through Private relay (relay.example.com) (encrypted).', 'zh', false),
    '通过私人中继（relay.example.com）随时随地连接（已加密）。',
  )
  assert.equal(translate('Use Anthropic Console (API usage billing)', 'zh', false), '使用 Anthropic 控制台（按 API 用量计费）')
  assert.equal(translate('12 pt (Default)', 'zh', false), '12 pt（默认）')
  assert.equal(
    translate("Claude Code isn't set up on this computer. Install Claude Code and run `claude auth login`.", 'zh', false),
    'Claude Code 还没有在这台电脑上设置好。安装 Claude Code，然后运行 `claude auth login`。',
  )
  assert.equal(
    translate("Claude Code isn't set up on this computer. Install Claude Code and run `claude auth login`.", 'en', true),
    "Claude Code isn't set up on this computer. Install Claude Code and run `claude auth login`.",
  )
})


test('each supported locale follows regional system tags and explicit choices win', () => {
  for (const language of ['ja', 'ko', 'fr', 'de', 'es'] as const) {
    assert.equal(resolvedLanguage('system', `${language.toUpperCase()}_XX.UTF-8`), language)
    assert.equal(resolvedLanguage(language, 'zh-TW'), language)
    assert.equal(usesChinese(language, 'zh-TW'), false)
    assert.notEqual(translate('Settings', language), 'Settings')
    assert.equal(translate('Settings', 'system', `${language}-XX`), translate('Settings', language))
    assert.equal(translate('An unknown phrase', language), 'An unknown phrase')
    assert.equal(translate('Remove {name}?', language).replaceAll('{name}', 'Done').includes('Done'), true)
    assert.equal(resetLine('resets 3h 20m', language).includes('3h 20m'), true)
    assert.notEqual(languageTitle(language, 'en', 'en'), 'Follow system')
  }
  assert.equal(new Set(appLanguages).size, 9)
  assert.equal(resolvedLanguage('system', 'pt-BR'), 'en')
  assert.equal(translate('Settings', 'en', 'ja-JP'), 'Settings')
})


test('English reset text remains literal and absolute dates do not become durations', () => {
  assert.equal(resetLine('resets Sep 26 at 12pm', 'en'), 'resets Sep 26 at 12pm')
  assert.equal(resetLine('resets 3h 20m', 'en'), 'resets 3h 20m')
  for (const language of ['ja', 'ko', 'fr', 'de', 'es'] as const) {
    assert.equal(resetLine('resets Sep 26 at 12pm', language), `${translate('resets', language)} Sep 26 at 12pm`)
  }
})
