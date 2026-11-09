import io
import os

entries = {
    'Last message just now': {
        'ar': 'آخر رسالة الآن',
        'de': 'Letzte Nachricht gerade eben',
        'es': 'Último mensaje ahora mismo',
        'fr': "Dernier message à l'instant",
        'ja': '最後のメッセージはたった今',
        'ko': '방금 마지막 메시지',
        'pt-BR': 'Última mensagem agora mesmo',
        'ru': 'Последнее сообщение только что',
        'zh-Hans': '最后一条消息刚刚收到',
        'zh-Hant': '最後一則訊息剛剛收到',
    },
    'Nothing received yet': {
        'ar': 'لم تُستلم أي رسالة بعد',
        'de': 'Noch nichts empfangen',
        'es': 'Aún no se recibió nada',
        'fr': 'Rien reçu pour le moment',
        'ja': 'まだ何も受信していません',
        'ko': '아직 받은 것이 없습니다',
        'pt-BR': 'Nada recebido ainda',
        'ru': 'Пока ничего не получено',
        'zh-Hans': '尚未收到任何消息',
        'zh-Hant': '尚未收到任何訊息',
    },
    'Pair someone': {
        'ar': 'اربط شخصًا',
        'de': 'Jemanden verbinden',
        'es': 'Vincular a alguien',
        'fr': 'Associer quelqu’un',
        'ja': '誰かをリンク',
        'ko': '사람 연결',
        'pt-BR': 'Vincular alguém',
        'ru': 'Привязать человека',
        'zh-Hans': '关联某人',
        'zh-Hant': '關聯某人',
    },
    'Slack is not connected.': {
        'ar': 'Slack غير متصل.',
        'de': 'Slack ist nicht verbunden.',
        'es': 'Slack no está conectado.',
        'fr': "Slack n'est pas connecté.",
        'ja': 'Slack は接続されていません。',
        'ko': 'Slack이 연결되지 않았습니다.',
        'pt-BR': 'O Slack não está conectado.',
        'ru': 'Slack не подключён.',
        'zh-Hans': 'Slack 未连接。',
        'zh-Hant': 'Slack 未連線。',
    },
    'That did not work.': {
        'ar': 'لم ينجح ذلك.',
        'de': 'Das hat nicht geklappt.',
        'es': 'No funcionó.',
        'fr': "Cela n'a pas marché.",
        'ja': 'うまくいきませんでした。',
        'ko': '잘 되지 않았습니다.',
        'pt-BR': 'Não funcionou.',
        'ru': 'Не получилось.',
        'zh-Hans': '没有成功。',
        'zh-Hant': '沒有成功。',
    },
}

root = os.path.join('apps', 'ios', 'Kit', 'Sources', 'CodyncKit', 'Resources', 'Languages')
for key, values in entries.items():
    prefix = key.split(' ')[0] + ' '
    for code, value in values.items():
        path = os.path.join(root, code + '.json')
        with io.open(path, encoding='utf-8') as handle:
            lines = handle.readlines()
        if any(line.strip().startswith('"' + key + '":') for line in lines):
            print(code, 'already', key[:18])
            continue
        for index, line in enumerate(lines):
            if line.strip().startswith('"' + prefix):
                indent = line[:len(line) - len(line.lstrip())]
                lines.insert(index, '%s"%s": "%s",\n' % (indent, key, value))
                break
        else:
            # A partial catalog (Arabic) may lack the anchor: append before the closing brace.
            last = max(index for index, line in enumerate(lines) if line.strip())
            indent = lines[last - 1][:len(lines[last - 1]) - len(lines[last - 1].lstrip())]
            lines.insert(last, '%s"%s": "%s"\n' % (indent, key, value))
            if not lines[last - 1].rstrip().endswith(','):
                lines[last - 1] = lines[last - 1].rstrip('\n').rstrip() + ',\n'
        with io.open(path, 'w', encoding='utf-8', newline='') as handle:
            handle.writelines(lines)
        print(code, 'added', key[:18])
