#!/usr/bin/env python3
"""Forwarding MITM CONNECT proxy: relays to the REAL upstream (so token/user
pre-checks succeed and the client proceeds to its model call) while passively
sniffing the client->upstream plaintext for the HTTP/2 fingerprint + UA + JA4.

Mirrors the CLIENT's negotiated ALPN to the upstream so the relay is consistent
(an http/1.1 client is not bridged onto an h2 upstream).

  python3 capture_fwd_mitm.py <port> <ca_out>
Env:
  UPSTREAM_PROXY   optional http://host:port to chain egress (e.g. the box's real proxy)
  REAL_CA_BUNDLE   optional CA file for verifying upstream (auto-detected otherwise)

Emits one `FP|<host>|<ja4>|<h1|h2>|...|UA=<ua>` line per first request.
  h1: FP|host|ja4|h1|-|<request-line>|UA=<ua>
  h2: FP|host|ja4|h2|<settings>|<window_update>|<priority>|<pseudo_order>|<path>|UA=<ua>
"""
import socket, ssl, sys, os, threading, tempfile, datetime, ipaddress
from cryptography import x509
from cryptography.x509.oid import NameOID
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from hyperframe.frame import Frame, SettingsFrame, WindowUpdateFrame, HeadersFrame, ContinuationFrame, PriorityFrame
import hpack

UPSTREAM_PROXY=os.environ.get("UPSTREAM_PROXY","").strip()
REAL_CA=os.environ.get("REAL_CA_BUNDLE")
if not REAL_CA:
    for c in ("/etc/ssl/certs/ca-certificates.crt","/etc/pki/tls/certs/ca-bundle.crt"):
        if os.path.exists(c): REAL_CA=c; break
if not REAL_CA:
    try: import certifi; REAL_CA=certifi.where()
    except Exception: REAL_CA=None

GREASE={0x0a0a,0x1a1a,0x2a2a,0x3a3a,0x4a4a,0x5a5a,0x6a6a,0x7a7a,0x8a8a,0x9a9a,0xaaaa,0xbaba,0xcaca,0xdada,0xeaea,0xfafa}
def _u16(b,i): return (b[i]<<8)|b[i+1]
def ja4_quick(peek):
    import hashlib as _h
    try:
        body=peek[5:5+_u16(peek,3)]
        if body[0]!=0x01: return "?"
        i=4; cv=_u16(body,i); i+=2; i+=32
        i+=1+body[i]                      # session id
        cl=_u16(body,i); i+=2; ciph=[_u16(body,i+j) for j in range(0,cl,2)]; i+=cl
        i+=1+body[i]                      # compression
        et=_u16(body,i); i+=2; end=i+et
        exts=[]; sni=False; alpns=[]; sig=[]; sv=[]
        while i<end:
            t=_u16(body,i); l=_u16(body,i+2); d=body[i+4:i+4+l]; i+=4+l; exts.append(t)
            if t==0: sni=True
            elif t==0x10:
                ll=_u16(d,0); k=2
                while k<2+ll: sl=d[k]; alpns.append(d[k+1:k+1+sl].decode('latin1')); k+=1+sl
            elif t==0x0d:
                n=_u16(d,0)
                for j in range(0,n,2): sig.append(_u16(d,2+j))
            elif t==0x2b:
                n=d[0]
                for j in range(0,n,2): sv.append(_u16(d,1+j))
        ng=lambda x:[v for v in x if v not in GREASE]
        VM={0x0304:"13",0x0303:"12"}; tv=VM.get(max(ng(sv) or [cv]),"00")
        nc=ng(ciph); ne=ng(exts)
        al=alpns[0] if alpns else ""; alpn=(al[0]+al[-1]) if len(al)>=2 else "00"
        a=f"t{tv}{'d' if sni else 'i'}{min(len(nc),99):02d}{min(len(ne),99):02d}{alpn}"
        b=_h.sha256(",".join(sorted("%04x"%c for c in nc)).encode()).hexdigest()[:12]
        ce=sorted("%04x"%e for e in ne if e not in (0,0x10))
        c=_h.sha256((",".join(ce)+"_"+",".join("%04x"%s for s in sig)).encode()).hexdigest()[:12]
        return f"{a}_{b}_{c}"
    except Exception: return "?"

LOCK=threading.Lock()
def log(m):
    with LOCK: print(m,flush=True)

def mk_ca():
    k=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    s=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,"fp-capture-ca")]); n0=datetime.datetime(2025,1,1)
    c=(x509.CertificateBuilder().subject_name(s).issuer_name(s).public_key(k.public_key()).serial_number(1)
       .not_valid_before(n0).not_valid_after(datetime.datetime(2035,1,1))
       .add_extension(x509.BasicConstraints(ca=True,path_length=None),True).sign(k,hashes.SHA256()))
    return k,c
def mk_leaf(host,ck,cc):
    k=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    s=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,host)])
    try: san=x509.IPAddress(ipaddress.ip_address(host))
    except ValueError: san=x509.DNSName(host)
    n0=datetime.datetime(2025,1,1)
    c=(x509.CertificateBuilder().subject_name(s).issuer_name(cc.subject).public_key(k.public_key())
       .serial_number(x509.random_serial_number()).not_valid_before(n0).not_valid_after(datetime.datetime(2035,1,1))
       .add_extension(x509.SubjectAlternativeName([san]),False).sign(ck,hashes.SHA256()))
    return k,c

def dial(host,port):
    if UPSTREAM_PROXY:
        u=UPSTREAM_PROXY.split("://")[-1]; ph,_,pp=u.partition(":")
        s=socket.create_connection((ph,int(pp or "8080")),timeout=15)
        s.sendall(f"CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n".encode())
        r=b""
        while b"\r\n\r\n" not in r:
            c=s.recv(4096)
            if not c: raise IOError("egress proxy closed")
            r+=c
        if b" 200 " not in r.split(b"\r\n")[0]: raise IOError("egress CONNECT failed: "+r.split(b"\r\n")[0].decode('latin1','replace'))
        return s
    return socket.create_connection((host,port),timeout=15)

PSEUDO={":method":"m",":authority":"a",":scheme":"s",":path":"p"}
class Sniffer:
    def __init__(self,proto,host,ja4="?"):
        self.proto=proto; self.host=host; self.ja4=ja4; self.buf=b""; self.done=False
        self.settings=None; self.wu="0"; self.prios=[]
        self.dec=hpack.Decoder(); self.hblock=b""; self.cont=False; self.started=False
    def feed(self,data):
        if self.done: return
        self.buf+=data
        try: (self._h2 if self.proto=="h2" else self._h1)()
        except Exception as e: log(f"  [{self.host}] sniff error: {e!r}"); self.done=True
    def _h1(self):
        if b"\r\n\r\n" in self.buf:
            head=self.buf.split(b"\r\n\r\n",1)[0].decode('latin1','replace')
            line=head.split("\r\n",1)[0]
            ua=next((l.split(":",1)[1].strip() for l in head.split("\r\n") if l.lower().startswith("user-agent:")),None)
            log(f"FP|{self.host}|{self.ja4}|h1|-|{line[:80]}|UA={ua}"); self.done=True
    def _h2(self):
        if not self.started:
            if len(self.buf)<24: return
            if not self.buf.startswith(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"): self.done=True; return
            self.buf=self.buf[24:]; self.started=True
        while len(self.buf)>=9:
            f,length=Frame.parse_frame_header(memoryview(self.buf[:9]))
            if len(self.buf)<9+length: break
            body=self.buf[9:9+length]; self.buf=self.buf[9+length:]
            try: f.parse_body(memoryview(body))
            except Exception: pass
            if isinstance(f,SettingsFrame) and 'ACK' not in f.flags and self.settings is None:
                self.settings=";".join(f"{int(k)}:{v}" for k,v in f.settings.items())
            elif isinstance(f,WindowUpdateFrame) and f.stream_id==0:
                self.wu=str(f.window_increment)
            elif isinstance(f,PriorityFrame):
                self.prios.append(f"{f.stream_id}:{1 if f.exclusive else 0}:{f.depends_on}:{f.stream_weight}")
            elif isinstance(f,HeadersFrame):
                self.hblock+=f.data
                if 'END_HEADERS' in f.flags: return self._emit()
                self.cont=True
            elif isinstance(f,ContinuationFrame) and self.cont:
                self.hblock+=f.data
                if 'END_HEADERS' in f.flags: return self._emit()
    def _emit(self):
        try: hl=self.dec.decode(self.hblock)
        except Exception: hl=[]
        pseudo=",".join(PSEUDO.get(n,n) for n,_ in hl if n.startswith(":"))
        ua=next((v for n,v in hl if n.lower()=="user-agent"),None)
        path=next((v for n,v in hl if n==":path"),"")
        log(f"FP|{self.host}|{self.ja4}|h2|{self.settings or ''}|{self.wu}|{','.join(self.prios) or '0'}|{pseudo}|{path[:60]}|UA={ua}")
        self.done=True

def pump(src,dst,sniff):
    try:
        while True:
            d=src.recv(65536)
            if not d: break
            if sniff: sniff.feed(d)
            dst.sendall(d)
    except Exception: pass
    finally:
        try: dst.shutdown(socket.SHUT_WR)
        except Exception: pass

def read_connect(conn):
    buf=b""
    while b"\r\n\r\n" not in buf:
        c=conn.recv(4096)
        if not c: break
        buf+=c
    tgt=buf.split(b"\r\n",1)[0].split(b" ")[1].decode()
    conn.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
    h,_,p=tgt.partition(":"); return h,int(p or "443")

def handle(client,ck,cc):
    client.settimeout(30)
    host,port=read_connect(client)
    log(f"[CONNECT {host}:{port}]")
    try: ja4=ja4_quick(client.recv(8192,socket.MSG_PEEK))
    except Exception: ja4="?"
    # 1) handshake with the CLIENT first, offering both, to learn ITS ALPN choice
    lk,lc=mk_leaf(host,ck,cc)
    sctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    with tempfile.NamedTemporaryFile("wb",delete=False,suffix=".pem") as cf:
        cf.write(lc.public_bytes(serialization.Encoding.PEM))
        cf.write(lk.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.TraditionalOpenSSL,serialization.NoEncryption()))
        cp=cf.name
    sctx.load_cert_chain(cp); os.unlink(cp)
    sctx.set_alpn_protocols(["h2","http/1.1"])
    try:
        cl=sctx.wrap_socket(client,server_side=True)
    except Exception as e:
        log(f"[{host}] client TLS failed: {e}"); client.close(); return
    proto=cl.selected_alpn_protocol() or "http/1.1"
    log(f"  [{host}] client TLS ok, client_alpn={proto}")
    # 2) connect upstream, mirroring the CLIENT's chosen protocol so the relay is consistent
    try:
        up_raw=dial(host,port)
    except Exception as e:
        log(f"[{host}:{port}] upstream dial failed: {e}"); cl.close(); return
    uctx=ssl.create_default_context(cafile=REAL_CA)
    uctx.set_alpn_protocols([proto])
    try:
        up=uctx.wrap_socket(up_raw,server_hostname=host)
    except Exception as e:
        log(f"[{host}] upstream TLS failed: {e}"); cl.close(); up_raw.close(); return
    log(f"  [{host}] upstream TLS ok, relaying ({proto})")
    cl.settimeout(60); up.settimeout(60)
    sn=Sniffer(proto,host,ja4)
    t=threading.Thread(target=pump,args=(cl,up,sn),daemon=True); t.start()
    pump(up,cl,None); t.join(timeout=1)
    try: cl.close()
    except Exception: pass
    try: up.close()
    except Exception: pass

def main():
    port=int(sys.argv[1]) if len(sys.argv)>1 else 8889
    ca_out=sys.argv[2] if len(sys.argv)>2 else "/tmp/fpca.pem"
    ck,cc=mk_ca(); open(ca_out,"wb").write(cc.public_bytes(serialization.Encoding.PEM))
    log(f"[fwd-MITM :{port} CA={ca_out} egress={UPSTREAM_PROXY or 'direct'} real_ca={REAL_CA}]")
    s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    s.bind(("127.0.0.1",port)); s.listen(16); s.settimeout(90)
    while True:
        try: conn,_=s.accept()
        except socket.timeout: log("[idle 90s, exiting]"); return
        threading.Thread(target=lambda c=conn:_safe(c,ck,cc),daemon=True).start()

def _safe(c,ck,cc):
    try: handle(c,ck,cc)
    except Exception as e:
        log(f"[handler err] {e!r}")
        try: c.close()
        except Exception: pass

if __name__=="__main__": main()
