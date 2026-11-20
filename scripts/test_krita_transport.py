"""Exercise the bundled Krita bridge without importing Krita or changing a profile."""
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import socket
from threading import Event, Thread
import time
import unittest


def read_exact(stream, size):
    data = bytearray()
    while len(data) < size:
        chunk = stream.recv(size - len(data))
        if not chunk:
            raise EOFError("Unexpected end of frame")
        data.extend(chunk)
    return bytes(data)


class KritaTransportTests(unittest.TestCase):
    def test_fragmented_handshake_and_bidirectional_color(self):
        module = Path(__file__).resolve().parents[1] / 'packaging/bridges/krita/okcolorpicker/transport.py'
        spec = importlib.util.spec_from_file_location('picker_transport', module)
        transport = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(transport)
        link = transport.ColorLink()
        stop = Event()
        client, server = socket.socketpair()
        with client, server:
            # socketpair is a TCP pair on Windows.
            client.settimeout(3)
            server.settimeout(3)
            worker = Thread(target=link.session, args=(client, stop), daemon=True)
            worker.start()
            try:
                request = bytearray()
                while b'\r\n\r\n' not in request:
                    request.extend(read_exact(server, 1))
                headers = dict(line.split(b': ', 1) for line in bytes(request).split(b'\r\n')[1:] if line)
                nonce = headers[b'Sec-WebSocket-Key']
                accept = base64.b64encode(hashlib.sha1(nonce + b'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest())
                server.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n')
                server.sendall(b'Connection: Upgrade\r\nSec-WebSocket-Accept: ' + accept + b'\r\n\r\n')
                payload = b'{"r":23,"g":45,"b":67,"slot":0}'
                server.sendall(bytes((1, 8)) + payload[:8])
                server.sendall(bytes((128, len(payload)-8)) + payload[8:])
                deadline = time.monotonic() + 3
                received = []
                while time.monotonic() < deadline and len(received) < 2:
                    received.extend(link.take())
                    time.sleep(0.01)
                self.assertEqual(received, [None, json.loads(payload)])
                link.send(1, (80, 90, 100))
                opcode, length = read_exact(server, 2)
                self.assertEqual(opcode, 129)
                self.assertTrue(length & 128)
                mask = read_exact(server, 4)
                data = read_exact(server, length & 127)
                self.assertEqual(json.loads(bytes(b ^ mask[i % 4] for i, b in enumerate(data))),
                                 dict(slot=1, r=80, g=90, b=100))
            finally:
                stop.set()
                worker.join(4)
            self.assertFalse(worker.is_alive())
