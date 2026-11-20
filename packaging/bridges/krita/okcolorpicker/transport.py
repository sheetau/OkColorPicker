"""A local WebSocket client for Krita's bundled Python (no extra modules required)."""
import base64
import hashlib
import json
import os
import select
import socket
import struct
from collections import deque
from threading import Lock


class ColorLink:
    def __init__(self):
        self.lock = Lock()
        self.incoming = deque(maxlen=64)
        self.outgoing = {}
        self.connected = False

    def take(self):
        with self.lock:
            messages = list(self.incoming)
            self.incoming.clear()
            return messages

    def send(self, slot, rgb):
        with self.lock:
            if self.connected:
                self.outgoing[slot] = dict(slot=slot, r=rgb[0], g=rgb[1], b=rgb[2])

    def run(self, stop):
        while not stop.is_set():
            try:
                with socket.create_connection(("127.0.0.1", 9011), timeout=1) as conn:
                    self.session(conn, stop)
            except (OSError, ValueError):
                pass
            finally:
                with self.lock:
                    self.connected = False
                    self.outgoing.clear()
                    self.incoming.clear()
            stop.wait(1)

    def session(self, conn, stop):
        nonce = base64.b64encode(os.urandom(16)).decode("ascii")
        conn.sendall(("GET / HTTP/1.1\r\nHost: 127.0.0.1:9011\r\nUpgrade: websocket\r\n"
                      "Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\n"
                      f"Sec-WebSocket-Key: {nonce}\r\n\r\n").encode("ascii"))
        data = bytearray()
        while b"\r\n\r\n" not in data:
            chunk = conn.recv(4096)
            if not chunk or len(data) > 8192:
                raise ValueError("Incomplete handshake")
            data.extend(chunk)
        headers, rest = data.split(b"\r\n\r\n", 1)
        lines = bytes(headers).split(b"\r\n")
        fields = {k.lower(): v.strip() for k, v in (line.split(b":", 1) for line in lines[1:])}
        expected = base64.b64encode(hashlib.sha1((nonce + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest())
        if lines[0].split()[1:2] != [b"101"] or fields.get(b"sec-websocket-accept") != expected:
            raise ValueError("Invalid WebSocket response")
        with self.lock:
            self.incoming.append(None)
            self.connected = True
        data = bytearray(rest)
        fragments = bytearray()
        partial = False
        while not stop.is_set():
            while len(data) >= 2:
                first, second = data[:2]
                size = second & 127
                header = 2
                if size == 126:
                    if len(data) < 4:
                        break
                    size = int.from_bytes(data[2:4], "big")
                    header = 4
                if second & 128 or (second & 127) == 127 or size > 16384 or first & 112:
                    raise ValueError("Unsupported frame")
                if len(data) < header + size:
                    break
                payload = bytes(data[header:header + size])
                del data[:header + size]
                opcode = first & 15
                if opcode == 8:
                    frame(conn, 8, payload)
                    return
                if opcode == 9:
                    frame(conn, 10, payload)
                    continue
                if opcode == 10:
                    continue
                if opcode not in (0, 1) or partial != (opcode == 0) or len(fragments) + size > 16384:
                    raise ValueError("Invalid text frame")
                fragments.extend(payload)
                partial = not bool(first & 128)
                if not partial:
                    message = json.loads(fragments)
                    fragments.clear()
                    if isinstance(message, dict):
                        with self.lock:
                            self.incoming.append(message)
            with self.lock:
                messages = list(self.outgoing.values())
                self.outgoing.clear()
            for message in messages:
                frame(conn, 1, json.dumps(message).encode())
            if select.select([conn], [], [], 0.035)[0]:
                chunk = conn.recv(4096)
                if not chunk:
                    return
                data.extend(chunk)
        frame(conn, 8, b"")


def frame(conn, opcode, payload):
    key = os.urandom(4)
    size = len(payload)
    header = bytes((128 | opcode, 128 | size)) if size < 126 else struct.pack("!BBH", 128 | opcode, 254, size)
    conn.sendall(header + key + bytes(value ^ key[i % 4] for i, value in enumerate(payload)))
