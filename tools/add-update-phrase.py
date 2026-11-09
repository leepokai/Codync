import io
import os

key = 'Update Codync on this computer to pair an Android phone.'
languages = {
    'ar': 'حدِّث Codync على هذا الحاسوب لربط هاتف Android.',
    'de': 'Aktualisiere Codync auf diesem Computer, um ein Android-Telefon zu verknüpfen.',
    'es': 'Actualiza Codync en esta computadora para vincular un teléfono Android.',
    'fr': 'Mettez à jour Codync sur cet ordinateur pour associer un téléphone Android.',
    'ja': 'Android の電話をリンクするには、このコンピューターの Codync を更新してください。',
    'ko': 'Android 휴대폰을 연결하려면 이 컴퓨터의 Codync를 업데이트하세요.',
    'pt-BR': 'Atualize o Codync neste computador para vincular um telefone Android.',
    'ru': 'Обновите Codync на этом компьютере, чтобы привязать телефон Android.',
    'zh-Hans': '更新这台电脑上的 Codync，以关联 Android 手机。',
    'zh-Hant': '更新這台電腦上的 Codync，以關聯 Android 手機。',
}
root = os.path.join('apps', 'ios', 'Kit', 'Sources', 'CodyncKit', 'Resources', 'Languages')
for code, value in languages.items():
    path = os.path.join(root, code + '.json')
    with io.open(path, encoding='utf-8') as handle:
        lines = handle.readlines()
    if any(line.strip().startswith('"' + key + '":') for line in lines):
        print(code, 'already')
        continue
    for index, line in enumerate(lines):
        if line.strip().startswith('"Update '):
            indent = line[:len(line) - len(line.lstrip())]
            lines.insert(index, '%s"%s": "%s",\n' % (indent, key, value))
            break
    else:
        # A partial catalog (Arabic) that ends before "Update": append it at the end.
        last = max(index for index, line in enumerate(lines) if line.strip())
        indent = lines[last - 1][:len(lines[last - 1]) - len(lines[last - 1].lstrip())]
        lines.insert(last, '%s"%s": "%s"\n' % (indent, key, value))
        if not lines[last - 1].rstrip().endswith(','):
            lines[last - 1] = lines[last - 1].rstrip('\n').rstrip() + ',\n'
    with io.open(path, 'w', encoding='utf-8', newline='') as handle:
        handle.writelines(lines)
    print(code, 'added')
