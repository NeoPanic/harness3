#!/usr/bin/env python3
"""Run with: python3 examples/plugins/http-reader/test_worker.py"""
import hashlib
import importlib.util
import json
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time
import unittest
from unittest.mock import patch
import urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parent


class FixtureHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/html")
            self.end_headers()
            return
        if self.path == "/slow":
            time.sleep(0.3)
        status = 404 if self.path == "/missing" else 200
        if self.path == "/html":
            body = "<html><style>.hidden { color:red }</style><h1>Harness³</h1><script>alert('unsafe')</script><p>证据 &amp; 来源</p></html>".encode()
            content_type = "text/html; charset=utf-8"
        elif self.path == "/large":
            body = b"x" * (256 * 1024 + 100)
            content_type = "text/plain"
        else:
            body = b"missing" if status == 404 else b"fixture evidence"
            content_type = "text/plain; charset=utf-8"
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *_):
        pass


class HTTPReaderTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = "http://127.0.0.1:" + str(cls.server.server_port)

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)

    def rpc(self, url):
        requests = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize"},
            {"jsonrpc": "2.0", "id": 2, "method": "health"},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read", "arguments": {"url": url}}},
            {"jsonrpc": "2.0", "id": 4, "method": "shutdown"},
        ]
        result = subprocess.run([sys.executable, str(ROOT / "worker.py")], input="".join(json.dumps(r) + "\n" for r in requests),
                                text=True, capture_output=True, timeout=15, check=True)
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(responses), 4)
        self.assertEqual(responses[0]["result"]["protocol"], "harness3-plugin-rpc/v1")
        self.assertEqual(responses[0]["result"]["id"], "http")
        self.assertTrue(responses[1]["result"]["healthy"])
        self.assertTrue(responses[3]["result"]["closed"])
        return responses[2]

    def test_manifest_digest_and_html_redirect_rpc(self):
        manifest = json.loads((ROOT / "manifest.json").read_text())
        self.assertEqual(hashlib.sha256((ROOT / "worker.py").read_bytes()).hexdigest(), manifest["files"]["worker.py"])
        response = self.rpc(self.base + "/redirect")
        self.assertFalse(response["result"]["changed"])
        content = response["result"]["content"]
        self.assertEqual(content["url"], self.base + "/html")
        self.assertEqual(content["status"], 200)
        self.assertIn("text/html", content["content_type"])
        self.assertIn("Harness³", content["text"])
        self.assertIn("证据 & 来源", content["text"])
        self.assertNotIn("unsafe", content["text"])
        self.assertNotIn("color:red", content["text"])
        self.assertFalse(content["truncated"])

    def test_size_limit_and_http_error_body(self):
        large = self.rpc(self.base + "/large")["result"]["content"]
        self.assertTrue(large["truncated"])
        self.assertEqual(len(large["text"].encode()), 256 * 1024)
        missing = self.rpc(self.base + "/missing")["result"]["content"]
        self.assertEqual(missing["status"], 404)
        self.assertEqual(missing["text"], "missing")

    def test_non_http_rejected_and_total_timeout_bound(self):
        self.assertEqual(self.rpc("file:///etc/hosts")["error"]["code"], "invalid_url")
        spec = importlib.util.spec_from_file_location("http_reader_example", ROOT / "worker.py")
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        worker.TIMEOUT_SECONDS = 0.05
        started = time.monotonic()
        with self.assertRaises(worker.ReadError) as raised:
            worker.read_url(self.base + "/slow")
        self.assertEqual(str(raised.exception), "http_timeout")
        self.assertLess(time.monotonic() - started, 0.2)

    def test_socket_timeout_classification_before_rpc_deadline(self):
        spec = importlib.util.spec_from_file_location("http_reader_timeout", ROOT / "worker.py")
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        for error in (socket.timeout("timed out"), urllib.error.URLError(socket.timeout("timed out"))):
            with self.subTest(error=type(error).__name__):
                with patch.object(worker, "fetch_url", side_effect=error):
                    with self.assertRaises(worker.ReadError) as raised:
                        worker.read_url(self.base + "/slow")
                self.assertEqual(str(raised.exception), "http_timeout")


if __name__ == "__main__":
    unittest.main()
