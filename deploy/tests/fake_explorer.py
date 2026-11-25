#!/usr/bin/env python3
"""A fake explorer for scan2-verify.test.sh: `fixed` is the build after SCAN-2, `old` the one before.

Prints its port on stdout, then serves until killed. Standard library only."""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

MODE = sys.argv[1]
OLD_CHUNK = 'function a(e){return r("/nullifiers/".concat(encodeURIComponent(e)))}function b(n){return p("/nullifiers/lookup",{nullifiers:n})}'
CHUNK = "/_next/static/chunks/app/%5Blang%5D/viewing/page-1a2b.js"
NEW_CHUNK = 'function a(e){return r("/nullifiers".concat(q({after:null!=e?e:void 0,limit:5e3})))}'


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, body, ctype="application/json"):
        b = body.encode()
        self.send_response(code)
        self.send_header("content-type", ctype)
        self.send_header("content-length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def redirect(self, to):
        self.send_response(308)
        self.send_header("location", to)
        self.send_header("content-length", "0")
        self.end_headers()

    def history_page(self):
        if MODE == "no-chunks":
            return self.send(200, "<html></html>", "text/html")
        page = "" if MODE == "other-page" else '<script src="%s"></script>' % CHUNK
        return self.send(200, '<html>%s<script src="/_next/static/chunks/main-9f.js"></script></html>' % page, "text/html")

    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0)))
        if self.path == "/api/v1/nullifiers/lookup" and MODE in ("old", "old-api"):
            return self.send(200, json.dumps({"spent": []}))
        self.send(405 if self.path.startswith("/api/v1/nullifiers") else 404, "{}")

    def do_GET(self):
        if self.path.startswith("/api/v1/nullifiers?"):
            if MODE == "no-page":
                return self.send(404, "{}")
            return self.send(200, json.dumps({"nullifiers": [], "next": None}))
        # Routing: `fixed`/`old*` serve the history page at /viewing and answer /en/viewing with a
        # 308 to it (the i18n build); `legacy-path` is a build from before that (/en/viewing only).
        host = "http://" + self.headers.get("host", "")
        if self.path in ("/viewing", "/en/viewing"):
            canonical = "/en/viewing" if MODE == "legacy-path" else "/viewing"
            if MODE == "redirect-offsite":
                return self.redirect("https://example.invalid/viewing")
            if MODE == "redirect-schemeless":
                return self.redirect("//example.invalid/viewing")
            if MODE == "redirect-home":
                return self.redirect(host + "/")
            if MODE == "redirect-loop":
                return self.redirect("/en/viewing" if self.path == "/viewing" else "/viewing")
            if MODE == "absolute-redirect" and self.path == "/viewing":
                return self.redirect(host + "/id/viewing")
            if self.path != canonical:
                if MODE == "legacy-path":
                    return self.send(404, "{}")
                return self.redirect(host + canonical)
            return self.history_page()
        if self.path == "/id/viewing" and MODE == "absolute-redirect":
            return self.history_page()
        if self.path == CHUNK:
            return self.send(200, OLD_CHUNK if MODE in ("old", "old-frontend") else NEW_CHUNK, "text/javascript")
        if self.path == "/_next/static/chunks/main-9f.js":
            return self.send(200, "console.log(1)", "text/javascript")
        self.send(404, "{}")


srv = HTTPServer(("127.0.0.1", 0), H)
print(srv.server_address[1], flush=True)
srv.serve_forever()
