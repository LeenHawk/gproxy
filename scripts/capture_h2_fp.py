#!/usr/bin/env python3
"""MITM that captures the HTTP/2 fingerprint (Akamai format) of a client.

Offers ALPN h2+http/1.1. For each connection it terminates TLS (temp CA) and:
  - h2  -> parses SETTINGS / WINDOW_UPDATE / PRIORITY (raw) + HPACK pseudo-header order
           => Akamai string: <settings>|<window_update>|<priority>|<pseudo_order>
  - h1  -> just prints request line + User-Agent
Loops over several connections so the model-path h2 connection is caught even
if preceded by http/1.1 calls. Also peeks the ClientHello for JA4 confirmation.

NOTE: this is the *terminating* MITM (returns a dummy 200). For clients that must
get a real upstream response before reaching their model call (gemini/copilot),
use capture_fwd_mitm.py instead.
"""
import socket, ssl, sys, hashlib, datetime, ipaddress, tempfile, os
from cryptography import x509
from cryptography.x509.oid import NameOID
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from hyperframe.frame import Frame, SettingsFrame, WindowUpdateFrame, PriorityFrame, HeadersFrame, ContinuationFrame
import hpack

GREASE={0x0a0a,0x1a1a,0x2a2a,0x3a3a,0x4a4a,0x5a5a,0x6a6a,0x7a7a,0x8a8a,0x9a9a,0xaaaa,0xbaba,0xcaca,0xdada,0xeaea,0xfafa}
def u16(b,i): return (b[i]<<8)|b[i+1]

def ja4_quick(peek):
    try:
        body=peek[5:5+u16(peek,3)]
        if body[0]!=0x01: return "?", []
        i=4; cv=u16(body,i); i+=2; i+=32
        sl=body[i]; i+=1+sl
        cl=u16(body,i); i+=2
        ciphers=[u16(body,i+j) for j in range(0,cl,2)]; i+=cl
        cm=body[i]; i+=1+cm
        et=u16(body,i); i+=2; end=i+et
        exts=[];sni=False;alpns=[];sig=[];sv=[]
        while i<end:
            t=u16(body,i);l=u16(body,i+2);d=body[i+4:i+4+l];i+=4+l;exts.append(t)
            if t==0x0000: sni=True
            elif t==0x0010:
                ll=u16(d,0);k=2
                while k<2+ll: sl2=d[k];alpns.append(d[k+1:k+1+sl2].decode('latin1'));k+=1+sl2
            elif t==0x000d:
                n=u16(d,0)
                for j in range(0,n,2):sig.append(u16(d,2+j))
            elif t==0x002b:
                n=d[0]
                for j in range(0,n,2):sv.append(u16(d,1+j))
        ng=lambda x:[v for v in x if v not in GREASE]
        VM={0x0304:"13",0x0303:"12"}; vers=ng(sv) or [cv]; tv=VM.get(max(vers),"00")
        nc=len(ng(ciphers)); ne=len(ng(exts))
        al=alpns[0] if alpns else ""; alpn=(al[0]+al[-1]) if len(al)>=2 else "00"
        return f"t{tv}{'d' if sni else 'i'}{min(nc,99):02d}{min(ne,99):02d}{alpn}", alpns
    except Exception: return "?", []

def mk_ca():
    key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    sub=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,"fp-capture-ca")])
    n0=datetime.datetime(2025,1,1)
    c=(x509.CertificateBuilder().subject_name(sub).issuer_name(sub).public_key(key.public_key())
       .serial_number(1).not_valid_before(n0).not_valid_after(datetime.datetime(2035,1,1))
       .add_extension(x509.BasicConstraints(ca=True,path_length=None),True).sign(key,hashes.SHA256()))
    return key,c
def mk_leaf(host,ca_key,ca_cert):
    key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    sub=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,host)])
    try: san=x509.IPAddress(ipaddress.ip_address(host))
    except ValueError: san=x509.DNSName(host)
    n0=datetime.datetime(2025,1,1)
    c=(x509.CertificateBuilder().subject_name(sub).issuer_name(ca_cert.subject).public_key(key.public_key())
       .serial_number(x509.random_serial_number()).not_valid_before(n0).not_valid_after(datetime.datetime(2035,1,1))
       .add_extension(x509.SubjectAlternativeName([san]),False).sign(ca_key,hashes.SHA256()))
    return key,c

PREFACE=b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"
PSEUDO={":method":"m",":authority":"a",":scheme":"s",":path":"p"}

def parse_h2(tconn):
    buf=b""
    settings_str=None; wu="0"; prios=[]; pseudo=None
    def fill(n):
        nonlocal buf
        while len(buf)<n:
            c=tconn.recv(8192)
            if not c: raise EOFError
            buf+=c
    fill(len(PREFACE))
    if not buf.startswith(PREFACE): return None
    buf=buf[len(PREFACE):]
    tconn.sendall(b'\x00\x00\x00\x04\x00\x00\x00\x00\x00')  # server preface: empty SETTINGS
    dec=hpack.Decoder(); hdr_block=b""; want_cont=False
    for _ in range(200):
        fill(9)
        f,length=Frame.parse_frame_header(memoryview(buf[:9]))
        fill(9+length)
        body=buf[9:9+length]; buf=buf[9+length:]
        try: f.parse_body(memoryview(body))
        except Exception: pass
        if isinstance(f,SettingsFrame):
            if 'ACK' not in f.flags:
                settings_str=";".join(f"{int(k)}:{v}" for k,v in f.settings.items())
                tconn.sendall(b'\x00\x00\x00\x04\x01\x00\x00\x00\x00')  # ACK
        elif isinstance(f,WindowUpdateFrame) and f.stream_id==0:
            wu=str(f.window_increment)
        elif isinstance(f,PriorityFrame):
            prios.append(f"{f.stream_id}:{1 if f.exclusive else 0}:{f.depends_on}:{f.stream_weight}")
        elif isinstance(f,HeadersFrame):
            hdr_block+=body
            if 'END_HEADERS' in f.flags:
                hl=dec.decode(hdr_block)
                pseudo=",".join(PSEUDO.get(n,n) for n,_ in hl if n.startswith(":"))
                ua=next((v for n,v in hl if n.lower()=="user-agent"),None)
                path=next((v for n,v in hl if n==":path"),"")
                return dict(settings=settings_str or "",wu=wu,prio=",".join(prios) or "0",pseudo=pseudo or "",ua=ua,path=path)
            want_cont=True
        elif isinstance(f,ContinuationFrame) and want_cont:
            hdr_block+=body
            if 'END_HEADERS' in f.flags:
                hl=dec.decode(hdr_block)
                pseudo=",".join(PSEUDO.get(n,n) for n,_ in hl if n.startswith(":"))
                ua=next((v for n,v in hl if n.lower()=="user-agent"),None)
                return dict(settings=settings_str or "",wu=wu,prio=",".join(prios) or "0",pseudo=pseudo or "",ua=ua,path="")
    return dict(settings=settings_str or "",wu=wu,prio=",".join(prios) or "0",pseudo="(no headers)",ua=None,path="")

def read_connect(conn):
    buf=b""
    while b"\r\n\r\n" not in buf:
        c=conn.recv(4096)
        if not c: break
        buf+=c
    host=buf.split(b"\r\n",1)[0].split(b" ")[1].decode() if b" " in buf else "?:0"
    conn.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
    return host.split(":")[0]

def handle(conn,ck,cc):
    host=read_connect(conn)
    try: peek=conn.recv(8192,socket.MSG_PEEK)
    except Exception: peek=b""
    j4,alpns=ja4_quick(peek) if peek else ("?",[])
    lk,lc=mk_leaf(host,ck,cc)
    ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    with tempfile.NamedTemporaryFile("wb",delete=False,suffix=".pem") as cf:
        cf.write(lc.public_bytes(serialization.Encoding.PEM))
        cf.write(lk.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.TraditionalOpenSSL,serialization.NoEncryption()))
        cp=cf.name
    ctx.load_cert_chain(cp); os.unlink(cp)
    ctx.set_alpn_protocols(["h2","http/1.1"])
    try: t=ctx.wrap_socket(conn,server_side=True)
    except ssl.SSLError as e:
        print(f"[{host}] TLS rejected: {e} (offered ALPN {alpns})",flush=True); return
    proto=t.selected_alpn_protocol()
    t.settimeout(8)
    print(f"\n[CONNECT {host}] JA4={j4} ALPN_offered={alpns} -> negotiated={proto}",flush=True)
    if proto=="h2":
        try:
            r=parse_h2(t)
            if r:
                print("  HTTP/2 Akamai :", f"{r['settings']}|{r['wu']}|{r['prio']}|{r['pseudo']}")
                print("  path          :", r['path'])
                print("  UA            :", r['ua'])
        except Exception as e:
            print("  [h2 parse err]",repr(e))
    else:
        try:
            req=b""
            while b"\r\n\r\n" not in req and len(req)<16384:
                c=t.recv(4096)
                if not c: break
                req+=c
            txt=req.decode('latin1','replace')
            print("  HTTP/1.1 req  :", txt.split('\r\n',1)[0][:100])
            ua=next((l.split(":",1)[1].strip() for l in txt.split("\r\n") if l.lower().startswith("user-agent:")),None)
            print("  UA            :", ua)
        except Exception: pass
    try: t.close()
    except Exception: pass

def main():
    port=int(sys.argv[1]) if len(sys.argv)>1 else 8888
    ca_out=sys.argv[2] if len(sys.argv)>2 else "/tmp/fpca.pem"
    ck,cc=mk_ca()
    open(ca_out,"wb").write(cc.public_bytes(serialization.Encoding.PEM))
    s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    s.bind(("127.0.0.1",port)); s.listen(8); s.settimeout(35)
    print(f"[CA {ca_out}; h2-MITM on 127.0.0.1:{port}]",flush=True)
    while True:
        try: conn,_=s.accept()
        except socket.timeout: print("\n[done/timeout]"); return
        try: handle(conn,ck,cc)
        except Exception as e:
            print("[err]",repr(e),flush=True)
            try: conn.close()
            except Exception: pass

if __name__=="__main__": main()
