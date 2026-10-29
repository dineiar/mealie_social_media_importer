#!/usr/bin/env python3
"""Run the real gallery-dl command line against a local server that answers every request with HTTP 429.

    local429.py -- --cookies FILE -o sleep-429=1 -- https://www.instagram.com/p/AAAAAAAAAAA/

Everything after the first `--` goes to gallery-dl. Run the script with the Python that has
gallery-dl installed. It sends every request that gallery-dl makes to www.instagram.com to a
server on 127.0.0.1 instead, so Instagram never sees a request. The server prints one stderr
line per request it receives, and the script prints the request count and gallery-dl's exit
code at the end. Hand it a throwaway cookies file with made-up values, because gallery-dl may
rewrite it.
"""

import http.server
import sys
import threading

import requests

hits = []


class AlwaysTooManyRequests(http.server.BaseHTTPRequestHandler):
    def respond(self):
        hits.append(self.path)
        print(f"stub: request {len(hits)} {self.command} {self.path.split('?')[0]}", file=sys.stderr, flush=True)
        self.send_response(429, "Too Many Requests")
        self.send_header("Content-Length", "0")
        self.end_headers()

    do_GET = do_HEAD = do_POST = respond

    def log_message(self, *args):
        pass


def main():
    argv = sys.argv[1:]
    if argv[:1] == ["--"]:
        argv = argv[1:]
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), AlwaysTooManyRequests)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    local = f"http://127.0.0.1:{server.server_port}/"

    send = requests.Session.request

    def redirected(self, method, url, **kwargs):
        return send(self, method, local + url.split("/", 3)[3], **kwargs)

    requests.Session.request = redirected

    import gallery_dl

    sys.argv = ["gallery-dl", *argv]
    try:
        code = gallery_dl.main()
    except SystemExit as exc:
        code = exc.code
    print(f"requests seen by the stub: {len(hits)}", file=sys.stderr)
    print(f"exit code: {code}", file=sys.stderr)
    sys.exit(code)


if __name__ == "__main__":
    main()
