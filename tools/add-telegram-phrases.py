import io
import os

entries = {
    'Add a Telegram bot, or remove one, on the computer: its token stays there.': {
        'ar': 'أضف بوت Telegram أو أزله على الحاسوب: يبقى رمزه هناك.',
        'de': 'Füge einen Telegram-Bot auf dem Computer hinzu oder entferne ihn dort: sein Token bleibt dort.',
        'es': 'Añade un bot de Telegram, o elimina uno, en la computadora: su token permanece allí.',
        'fr': "Ajoutez un bot Telegram sur l'ordinateur, ou retirez-en un : son jeton reste là-bas.",
        'ja': 'Telegram ボットの追加や削除はコンピューターで行います。トークンはそこに留まります。',
        'ko': 'Telegram 봇 추가나 제거는 컴퓨터에서 합니다. 토큰은 거기에 남습니다.',
        'pt-BR': 'Adicione um bot do Telegram, ou remova um, no computador: o token fica lá.',
        'ru': 'Добавьте или удалите бота Telegram на компьютере: его токен останется там.',
        'zh-Hans': '在电脑上添加或移除 Telegram 机器人：其令牌留在那里。',
        'zh-Hant': '在電腦上新增或移除 Telegram 機器人：其權杖留在那裡。',
    },
    'No Telegram bots yet.': {
        'ar': 'لا توجد بوتات Telegram بعد.',
        'de': 'Noch keine Telegram-Bots.',
        'es': 'Aún no hay bots de Telegram.',
        'fr': 'Pas encore de bots Telegram.',
        'ja': 'Telegram ボットはまだありません。',
        'ko': '아직 Telegram 봇이 없습니다.',
        'pt-BR': 'Ainda não há bots do Telegram.',
        'ru': 'Ботов Telegram пока нет.',
        'zh-Hans': '还没有 Telegram 机器人。',
        'zh-Hant': '還沒有 Telegram 機器人。',
    },
}
root = os.path.join('apps', 'ios', 'Kit', 'Sources', 'CodyncKit', 'Resources', 'Languages')
for key, values in entries.items():
    anchor = key[:key.index(' ') + 1]
    for code, value in values.items():
        path = os.path.join(root, code + '.json')
        with io.open(path, encoding='utf-8') as handle:
            lines = handle.readlines()
        if any(line.strip().startswith('"' + key + '":') for line in lines):
            print(code, 'already', key[:20])
            continue
        for index, line in enumerate(lines):
            if line.strip().startswith('"' + anchor):
                indent = line[:len(line) - len(line.lstrip())]
                lines.insert(index, '%s"%s": "%s",\n' % (indent, key, value))
                break
        else:
            raise SystemExit('no anchor for %s in %s' % (key[:20], path))
        with io.open(path, 'w', encoding='utf-8', newline='') as handle:
            handle.writelines(lines)
        print(code, 'added', key[:20])
