import re
import subprocess
import sys
import xml.etree.ElementTree as ET

subprocess.run(['adb', 'shell', 'uiautomator', 'dump', '/sdcard/z.xml'], capture_output=True)
raw = subprocess.run(['adb', 'shell', 'cat', '/sdcard/z.xml'], capture_output=True).stdout.decode('utf-8', 'replace')
raw = raw[raw.find('<?xml'):]
root = ET.fromstring(raw)

want = sys.argv[1] if len(sys.argv) > 1 else None


def bounds(node):
    value = node.get('bounds') or ''
    found = re.findall(r'\d+', value)
    return [int(v) for v in found] if len(found) == 4 else None


def walk(node, chain):
    chain = chain + [node]
    text = node.get('text') or node.get('content-desc') or ''
    if text:
        box = None
        for ancestor in reversed(chain):
            if ancestor.get('clickable') == 'true' and bounds(ancestor):
                box = bounds(ancestor)
                break
        if want is None or want.lower() in text.lower():
            target = box or bounds(node)
            center = ((target[0] + target[2]) // 2, (target[1] + target[3]) // 2) if target else None
            print(repr(text[:80]), center, 'clickable' if box else '')
    for child in node:
        walk(child, chain)


walk(root, [])
