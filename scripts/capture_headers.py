#!/usr/bin/env python3
"""Forwarding MITM that dumps the FULL ordered request header set per request
(not just UA), for collecting each agent CLI's impersonation header set. Same
relay + ALPN-mirroring as capture_fwd_mitm.py. Emits, per first request of each
connection:

  HDR\t<host>\t<h1|h2>\t<method> <path>
  H\t<name>\t<value>
  END

Group by (host, method, path-without-query) across samples to tell static
headers (constant value) from dynamic ones (per-request: uuids, session ids).
"""
import socket, ssl, sys, os, threading, tempfile, datetime, ipaddress
from cryptography import x509
from cryptography.x509.oid import NameOID
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from hyperframe.frame import Frame, HeadersFrame, ContinuationFrame
import hpack

UPSTREAM_PROXY = os.environ.get("UPSTREAM_PROXY", "").strip()
REAL_CA = os.environ.get("REAL_CA_BUNDLE")
if not REAL_CA:
    for c in ("/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt"):
        if os.path.exists(c):
            REAL_CA = c
            break
if not REAL_CA:
    try:
        import certifi
        REAL_CA = certifi.where()
    except Exception:
        REAL_CA = None

LOCK = threading.Lock()
def log(m):
    with LOCK:
        print(m, flush=True)

def mk_ca():
    k = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    s = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "fp-capture-ca")])
    n0 = datetime.datetime(2025, 1, 1)
    c = (x509.CertificateBuilder().subject_name(s).issuer_name(s).public_key(k.public_key())
         .serial_number(1).not_valid_before(n0).not_valid_after(datetime.datetime(2035, 1, 1))
         .add_extension(x509.BasicConstraints(ca=True, path_length=None), True).sign(k, hashes.SHA256()))
    return k, c
def mk_leaf(host, ck, cc):
    k = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    s = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, host)])
    try:
        san = x509.IPAddress(ipaddress.ip_address(host))
    except ValueError:
        san = x509.DNSName(host)
    n0 = datetime.datetime(2025, 1, 1)
    c = (x509.CertificateBuilder().subject_name(s).issuer_name(cc.subject).public_key(k.public_key())
         .serial_number(x509.random_serial_number()).not_valid_before(n0).not_valid_after(datetime.datetime(2035, 1, 1))
         .add_extension(x509.SubjectAlternativeName([san]), False).sign(ck, hashes.SHA256()))
    return k, c

def dial(host, port):
    if UPSTREAM_PROXY:
        u = UPSTREAM_PROXY.split("://")[-1]
        ph, _, pp = u.partition(":")
        s = socket.create_connection((ph, int(pp or "8080")), timeout=15)
        s.sendall(f"CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n".encode())
        r = b""
        while b"\r\n\r\n" not in r:
            c = s.recv(4096)
            if not c:
                raise IOError("egress proxy closed")
            r += c
        if b" 200 " not in r.split(b"\r\n")[0]:
            raise IOError("egress CONNECT failed")
        return s
    return socket.create_connection((host, port), timeout=15)

class Sniffer:
    def __init__(self, proto, host):
        self.proto = proto
        self.host = host
        self.buf = b""
        self.done = False
        self.dec = hpack.Decoder()
        self.hblock = b""
        self.cont = False
        self.started = False
    def feed(self, data):
        if self.done:
            return
        self.buf += data
        try:
            (self._h2 if self.proto == "h2" else self._h1)()
        except Exception as e:
            log(f"# sniff error {self.host}: {e!r}")
            self.done = True
    def _emit(self, method, path, pairs):
        out = [f"HDR\t{self.host}\t{self.proto}\t{method} {path}"]
        for n, v in pairs:
            out.append(f"H\t{n}\t{v}")
        out.append("END")
        log("\n".join(out))
        self.done = True
    def _h1(self):
        if b"\r\n\r\n" in self.buf:
            head = self.buf.split(b"\r\n\r\n", 1)[0].decode("latin1", "replace")
            lines = head.split("\r\n")
            req = lines[0].split(" ")
            method, path = (req[0], req[1]) if len(req) >= 2 else ("?", "?")
            pairs = []
            for ln in lines[1:]:
                if ":" in ln:
                    n, v = ln.split(":", 1)
                    pairs.append((n.strip(), v.strip()))
            self._emit(method, path, pairs)
    def _h2(self):
        if not self.started:
            if len(self.buf) < 24:
                return
            if not self.buf.startswith(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"):
                self.done = True
                return
            self.buf = self.buf[24:]
            self.started = True
        while len(self.buf) >= 9:
            f, length = Frame.parse_frame_header(memoryview(self.buf[:9]))
            if len(self.buf) < 9 + length:
                break
            body = self.buf[9:9 + length]
            self.buf = self.buf[9 + length:]
            try:
                f.parse_body(memoryview(body))
            except Exception:
                pass
            if isinstance(f, HeadersFrame):
                self.hblock += f.data
                if "END_HEADERS" in f.flags:
                    return self._emit_h2()
                self.cont = True
            elif isinstance(f, ContinuationFrame) and self.cont:
                self.hblock += f.data
                if "END_HEADERS" in f.flags:
                    return self._emit_h2()
    def _emit_h2(self):
        try:
            hl = self.dec.decode(self.hblock)
        except Exception:
            hl = []
        method = next((v for n, v in hl if n == ":method"), "?")
        path = next((v for n, v in hl if n == ":path"), "?")
        # pseudo-headers + regular headers, in order
        pairs = [(n, v) for n, v in hl]
        self._emit(method, path, pairs)

def pump(src, dst, sniff):
    try:
        while True:
            d = src.recv(65536)
            if not d:
                break
            if sniff:
                sniff.feed(d)
            dst.sendall(d)
    except Exception:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except Exception:
            pass

def read_connect(conn):
    buf = b""
    while b"\r\n\r\n" not in buf:
        c = conn.recv(4096)
        if not c:
            break
        buf += c
    tgt = buf.split(b"\r\n", 1)[0].split(b" ")[1].decode()
    conn.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
    h, _, p = tgt.partition(":")
    return h, int(p or "443")

def handle(client, ck, cc):
    client.settimeout(30)
    host, port = read_connect(client)
    lk, lc = mk_leaf(host, ck, cc)
    sctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    with tempfile.NamedTemporaryFile("wb", delete=False, suffix=".pem") as cf:
        cf.write(lc.public_bytes(serialization.Encoding.PEM))
        cf.write(lk.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL, serialization.NoEncryption()))
        cp = cf.name
    sctx.load_cert_chain(cp)
    os.unlink(cp)
    sctx.set_alpn_protocols(["h2", "http/1.1"])
    try:
        cl = sctx.wrap_socket(client, server_side=True)
    except Exception:
        client.close()
        return
    proto = cl.selected_alpn_protocol() or "http/1.1"
    try:
        up_raw = dial(host, port)
    except Exception as e:
        log(f"# dial fail {host}: {e}")
        cl.close()
        return
    uctx = ssl.create_default_context(cafile=REAL_CA)
    uctx.set_alpn_protocols([proto])
    try:
        up = uctx.wrap_socket(up_raw, server_hostname=host)
    except Exception as e:
        log(f"# upstream tls fail {host}: {e}")
        cl.close()
        up_raw.close()
        return
    cl.settimeout(60)
    up.settimeout(60)
    sn = Sniffer(proto, host)
    t = threading.Thread(target=pump, args=(cl, up, sn), daemon=True)
    t.start()
    pump(up, cl, None)
    t.join(timeout=1)
    for s in (cl, up):
        try:
            s.close()
        except Exception:
            pass

def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8889
    ca_out = sys.argv[2] if len(sys.argv) > 2 else "/tmp/fpca.pem"
    ck, cc = mk_ca()
    open(ca_out, "wb").write(cc.public_bytes(serialization.Encoding.PEM))
    log(f"# header-MITM :{port} CA={ca_out} egress={UPSTREAM_PROXY or 'direct'}")
    s = socket.socket()
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", port))
    s.listen(16)
    s.settimeout(90)
    while True:
        try:
            conn, _ = s.accept()
        except socket.timeout:
            log("# idle, exiting")
            return
        threading.Thread(target=lambda c=conn: handle(c, ck, cc), daemon=True).start()

if __name__ == "__main__":
    main()
