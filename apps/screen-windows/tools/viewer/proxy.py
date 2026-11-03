import http.server, urllib.request, sys, os
TOKEN = open(sys.argv[1]).read().strip()
UPSTREAM = sys.argv[2]
HERE = os.path.dirname(os.path.abspath(__file__))
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = open(os.path.join(HERE, 'viewer.html'), 'rb').read()
        self.send_response(200); self.send_header('Content-Type', 'text/html'); self.end_headers(); self.wfile.write(body)
    def do_POST(self):
        n = int(self.headers.get('Content-Length', 0)); data = self.rfile.read(n)
        if self.path.startswith('/report'):
            import json, base64
            r = json.loads(data)
            name = r.get('name', 'report')
            if r.get('png'):
                open(os.path.join(HERE, name + '.png'), 'wb').write(base64.b64decode(r.pop('png').split(',')[1]))
            open(os.path.join(HERE, name + '.json'), 'w').write(json.dumps(r, indent=1))
            self.send_response(200); self.end_headers(); self.wfile.write(b'{}'); return
        req = urllib.request.Request(UPSTREAM + self.path, data=data, method='POST', headers={'Authorization': 'Bearer ' + TOKEN, 'Content-Type': 'application/json'})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                out, code = r.read(), r.status
        except urllib.error.HTTPError as e:
            out, code = e.read(), e.code
        self.send_response(code); self.send_header('Content-Type', 'application/json'); self.end_headers(); self.wfile.write(out)
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(('127.0.0.1', 8099), H).serve_forever()
