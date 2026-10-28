#!/usr/bin/python3
"""Harness³ HTTP reader example; Python standard library, JSON-RPC over stdio."""
import json
import queue
import socket
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from html.parser import HTMLParser
from pathlib import Path

MAX_BYTES = 256 * 1024
TIMEOUT_SECONDS = 10


class ReadError(Exception):
    pass


def validate_url(url):
    if not isinstance(url, str) or len(url) > 8000:
        raise ReadError("invalid_arguments")
    try:
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme not in ("http", "https") or not parsed.hostname:
            raise ReadError("invalid_url")
        parsed.port
    except ValueError as error:
        raise ReadError("invalid_url") from error
    return url


class HTTPRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, newurl):
        validate_url(newurl)
        return super().redirect_request(request, fp, code, message, headers, newurl)


class HTMLText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hidden = 0
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag in ("script", "style"):
            self.hidden += 1
        elif not self.hidden and tag in ("p", "div", "br", "li", "h1", "h2", "h3", "tr"):
            self.parts.append("\n")

    def handle_endtag(self, tag):
        if tag in ("script", "style") and self.hidden:
            self.hidden -= 1
        elif not self.hidden and tag in ("p", "div", "li", "h1", "h2", "h3", "tr"):
            self.parts.append("\n")

    def handle_data(self, data):
        if not self.hidden:
            self.parts.append(data)

    def text(self):
        return "\n".join(line for line in (" ".join(s.split()) for s in "".join(self.parts).splitlines()) if line)


def fetch_url(url):
    deadline = time.monotonic() + TIMEOUT_SECONDS
    opener = urllib.request.build_opener(HTTPRedirects())
    request = urllib.request.Request(url, headers={
        "User-Agent": "Harness3-HTTP-Reader/1.0",
        "Accept": "text/html, text/plain, application/json, */*;q=0.1",
        "Accept-Encoding": "identity",
    }, method="GET")
    try:
        response = opener.open(request, timeout=TIMEOUT_SECONDS)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        raw = bytearray()
        while len(raw) <= MAX_BYTES:
            if time.monotonic() >= deadline:
                raise ReadError("http_timeout")
            block = response.read1(min(65536, MAX_BYTES + 1 - len(raw)))
            if not block:
                break
            raw.extend(block)
        truncated = len(raw) > MAX_BYTES
        content_type = response.headers.get("Content-Type", "application/octet-stream")
        encoding = response.headers.get_content_charset() or "utf-8"
        try:
            text = bytes(raw[:MAX_BYTES]).decode(encoding, errors="replace")
        except LookupError:
            text = bytes(raw[:MAX_BYTES]).decode("utf-8", errors="replace")
        if response.headers.get_content_type() in ("text/html", "application/xhtml+xml"):
            parser = HTMLText()
            parser.feed(text)
            text = parser.text()
        return {"url": response.geturl(), "status": response.status,
                "content_type": content_type, "text": text, "truncated": truncated}


def read_url(url):
    """Bound the complete request, including slow streaming, at the RPC boundary."""
    validate_url(url)
    result = queue.Queue(maxsize=1)

    def fetch():
        try:
            result.put((True, fetch_url(url)))
        except Exception as error:
            result.put((False, error))

    threading.Thread(target=fetch, daemon=True).start()
    try:
        ok, value = result.get(timeout=TIMEOUT_SECONDS)
    except queue.Empty as error:
        raise ReadError("http_timeout") from error
    if not ok:
        if isinstance(value, ReadError):
            raise value
        if isinstance(value, (TimeoutError, socket.timeout)):
            raise ReadError("http_timeout") from value
        if isinstance(value, urllib.error.URLError):
            if isinstance(value.reason, (TimeoutError, socket.timeout)):
                raise ReadError("http_timeout") from value
            raise ReadError("http_request_failed") from value
        raise ReadError("http_read_failed") from value
    return value


def main():
    with Path(__file__).with_name("manifest.json").open(encoding="utf-8") as source:
        manifest = json.load(source)
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        method = request["method"]
        try:
            if method == "initialize":
                result = {"protocol": "harness3-plugin-rpc/v1", "id": manifest["id"],
                          "version": manifest["version"], "tools": manifest["tools"]}
            elif method == "health":
                result = {"healthy": True}
            elif method == "shutdown":
                result = {"closed": True}
            elif method == "tools/call":
                params = request.get("params", {})
                arguments = params.get("arguments", {})
                if params.get("name") != "read" or not isinstance(arguments, dict) or set(arguments) != {"url"}:
                    raise ReadError("invalid_arguments")
                result = {"content": read_url(arguments["url"]), "changed": False}
            else:
                raise ReadError("unknown_method")
            reply = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        except ReadError as error:
            reply = {"jsonrpc": "2.0", "id": request["id"], "error": {"code": str(error)}}
        print(json.dumps(reply, ensure_ascii=False), flush=True)
        if method == "shutdown":
            break


if __name__ == "__main__":
    main()
