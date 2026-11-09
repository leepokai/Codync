import json
import sqlite3
import sys
import urllib.request

db = r'C:\Users\Edwin Villeda\.codync-dev\codync.db'
token = sqlite3.connect(db).execute("select v from kv where k='token'").fetchone()[0]
method = sys.argv[1]
body = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
req = urllib.request.Request(
    'http://127.0.0.1:19223/api/' + method,
    data=json.dumps(body).encode(),
    headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
)
try:
    print(urllib.request.urlopen(req).read().decode())
except urllib.error.HTTPError as error:
    print(error.code, error.read().decode())
