import re
import sys
import xml.etree.ElementTree as ET
from subprocess import run

text = sys.argv[1]
run(['adb', 'shell', 'uiautomator', 'dump', '/sdcard/p.xml'], capture_output=True)
xml = run(['adb', 'shell', 'cat', '/sdcard/p.xml'], capture_output=True, text=True).stdout
root = ET.fromstring(xml.split('\n', 1)[-1].strip() or xml)

parents = {child: parent for parent in root.iter() for child in parent}


def center(node):
    x1, y1, x2, y2 = map(int, re.findall(r'-?\d+', node.get('bounds')))
    return (x1 + x2) // 2, (y1 + y2) // 2


hits = [n for n in root.iter('node') if n.get('text') == text or n.get('content-desc') == text]
if not hits:
    print('no match')
    raise SystemExit(1)
node = hits[0]
while node is not None and node.get('clickable') != 'true':
    node = parents.get(node)
target = node or hits[0]
x, y = center(target)
print(x, y)
if '--tap' in sys.argv:
    run(['adb', 'shell', 'input', 'tap', str(x), str(y)])
