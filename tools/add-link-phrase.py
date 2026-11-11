import io
import os

languages = {
    'ar': 'ربط',
    'de': 'Verknüpfen',
    'es': 'Vincular',
    'fr': 'Associer',
    'ja': 'リンク',
    'ko': '연결',
    'pt-BR': 'Vincular',
    'ru': 'Привязать',
    'zh-Hans': '关联',
    'zh-Hant': '關聯',
}
root = os.path.join('apps', 'ios', 'Kit', 'Sources', 'CodyncKit', 'Resources', 'Languages')
for code, word in languages.items():
    path = os.path.join(root, code + '.json')
    with io.open(path, encoding='utf-8') as handle:
        lines = handle.readlines()
    if any(line.strip().startswith('"Link":') for line in lines):
        print(code, 'already')
        continue
    for index, line in enumerate(lines):
        if line.strip().startswith('"Link with a code":') or line.strip().startswith('"Link WhatsApp":'):
            indent = line[:len(line) - len(line.lstrip())]
            lines.insert(index, '%s"Link": "%s",\n' % (indent, word))
            break
    else:
        raise SystemExit('no anchor in ' + path)
    with io.open(path, 'w', encoding='utf-8', newline='') as handle:
        handle.writelines(lines)
    print(code, 'added')
