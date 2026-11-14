"""A dev server for tests: listens on `port_server.py <port>` until stopped. Not `python -m http.server`, which looks up
its own name before it listens: on CI's macOS machines that takes 35 s."""

import socket
import sys

listener = socket.create_server(("127.0.0.1", int(sys.argv[1])))
try:
    while True:
        connection, _ = listener.accept()
        connection.close()
except KeyboardInterrupt:
    pass
