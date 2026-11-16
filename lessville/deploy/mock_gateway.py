#!/usr/bin/env python3
"""Local stand-in for the Lessville Worker's /internal/llm and /internal/ckpt routes.

Mirrors the Worker's contract (key header, gzip check, object-name regex, 429 on budget)
so the agent service's model and checkpoint paths can be tested before deploying.
"""
import json, os, re, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

KEY = os.environ["MOCK_KEY"]
CAP = float(os.environ.get("MOCK_CAP", "40"))
STORE = os.environ.get("MOCK_STORE", "/tmp/mock-r2")
os.makedirs(STORE, exist_ok=True)
used = 0.0
calls = []


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype="application/json"):
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", ctype)
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _auth(self):
        if self.headers.get("x-lessville-key") != KEY:
            self._send(404, {"error": "not found"})
            return False
        return True

    def do_GET(self):
        if self.path == "/stats":
            return self._send(200, {"used": used, "cap": CAP, "calls": calls[-50:], "n": len(calls)})
        if not self._auth():
            return
        m = re.fullmatch(r"/internal/ckpt/((latest|hourly/\d{1,10})\.tar\.gz)", self.path)
        if not m:
            return self._send(400, {"error": "bad object"})
        p = os.path.join(STORE, m.group(1).replace("/", "_"))
        if not os.path.exists(p):
            return self._send(404, {"error": "not found"})
        self._send(200, open(p, "rb").read(), "application/gzip")

    def do_PUT(self):
        if not self._auth():
            return
        m = re.fullmatch(r"/internal/ckpt/((latest|hourly/\d{1,10})\.tar\.gz)", self.path)
        n = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(n)
        if not m:
            return self._send(400, {"error": "bad object"})
        if body[:2] != b"\x1f\x8b":
            return self._send(400, {"error": "not gzip"})
        open(os.path.join(STORE, m.group(1).replace("/", "_")), "wb").write(body)
        self._send(201, {"ok": True, "bytes": len(body)})

    def do_POST(self):
        global used
        if not self._auth():
            return
        n = int(self.headers.get("content-length", "0"))
        req = json.loads(self.rfile.read(n))
        msgs = req["messages"]
        assert len(msgs) == 2 and msgs[0]["role"] == "system" and msgs[1]["role"] == "user"
        chars = sum(len(m["content"]) for m in msgs)
        pt, ct = chars // 3, min(int(req["max_tokens"]), 60) // 2
        neurons = pt * 4119e-6 + ct * 34868e-6
        if used + neurons > CAP:
            return self._send(429, {"error": "daily neuron budget reached", "budget": {"used": used, "cap": CAP}})
        used += neurons
        calls.append({"task": req["task"], "agent": req["agent"], "chars": chars, "user": msgs[1]["content"]})
        if req["task"] == "phrase":
            text = "Morning! Same as usual, please."
        else:
            text = "Today I learned the bus can run late when it rains, so I should leave earlier."
        self._send(200, {"text": text, "usage": {"prompt_tokens": pt, "completion_tokens": ct}, "neurons": neurons, "budget": {"used": used, "cap": CAP}})


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
