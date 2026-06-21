#!/usr/bin/env python3
"""MITM CONNECT proxy: peek ClientHello (JA3/JA4) + terminate TLS to read User-Agent.

Generates an in-memory CA (written to --ca-out). Point a client's *_proxy env at it
AND trust the CA via NODE_EXTRA_CA_CERTS / SSL_CERT_FILE / etc.
"""
import socket, ssl, sys, hashlib, datetime, ipaddress, tempfile, os
from cryptography import x509
from cryptography.x509.oid import NameOID
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa

GREASE={0x0a0a,0x1a1a,0x2a2a,0x3a3a,0x4a4a,0x5a5a,0x6a6a,0x7a7a,0x8a8a,0x9a9a,0xaaaa,0xbaba,0xcaca,0xdada,0xeaea,0xfafa}
def u16(b,i): return (b[i]<<8)|b[i+1]

def parse(body):
    if not body or body[0]!=0x01: raise ValueError("not ClientHello")
    i=4; cv=u16(body,i); i+=2; i+=32
    sl=body[i]; i+=1+sl
    cl=u16(body,i); i+=2
    ciphers=[u16(body,i+j) for j in range(0,cl,2)]; i+=cl
    cm=body[i]; i+=1+cm
    et=u16(body,i); i+=2; end=i+et
    exts=[];curves=[];ecpf=[];sni=False;host=None;alpns=[];sig=[];sv=[]
    while i<end:
        t=u16(body,i);l=u16(body,i+2);d=body[i+4:i+4+l];i+=4+l;exts.append(t)
        if t==0x000a:
            n=u16(d,0)
            for j in range(0,n,2):curves.append(u16(d,2+j))
        elif t==0x000b:
            for j in range(d[0]):ecpf.append(d[1+j])
        elif t==0x0000:
            sni=True
            try:
                ln=u16(d,0);typ=d[2];hl=u16(d,3);host=d[5:5+hl].decode()
            except Exception:pass
        elif t==0x0010:
            ll=u16(d,0);k=2
            while k<2+ll:
                sl2=d[k];alpns.append(d[k+1:k+1+sl2].decode('latin1'));k+=1+sl2
        elif t==0x000d:
            n=u16(d,0)
            for j in range(0,n,2):sig.append(u16(d,2+j))
        elif t==0x002b:
            n=d[0]
            for j in range(0,n,2):sv.append(u16(d,1+j))
    return dict(cv=cv,ciphers=ciphers,exts=exts,curves=curves,ecpf=ecpf,sni=sni,host=host,alpns=alpns,sig=sig,sv=sv)

def ng(x):return [v for v in x if v not in GREASE]
def ja3(p):
    f=lambda xs:"-".join(str(x) for x in ng(xs))
    s=f"{p['cv']},{f(p['ciphers'])},{f(p['exts'])},{f(p['curves'])},{f(p['ecpf'])}"
    return s,hashlib.md5(s.encode()).hexdigest()
VM={0x0304:"13",0x0303:"12",0x0302:"11",0x0301:"10"}
def ja4(p):
    vers=ng(p['sv']) or [p['cv']];tv=VM.get(max(vers),"00")
    nc=ng(p['ciphers']);ne=ng(p['exts'])
    al=p['alpns'][0] if p['alpns'] else ""
    alpn=(al[0]+al[-1]) if len(al)>=2 else (al[0]+al[0]) if al else "00"
    a=f"t{tv}{'d' if p['sni'] else 'i'}{min(len(nc),99):02d}{min(len(ne),99):02d}{alpn}"
    cs=sorted("%04x"%c for c in nc)
    b=hashlib.sha256(",".join(cs).encode()).hexdigest()[:12] if cs else "000000000000"
    ce=sorted("%04x"%e for e in ne if e not in (0,0x10))
    c=hashlib.sha256((",".join(ce)+"_"+",".join("%04x"%s for s in p['sig'])).encode()).hexdigest()[:12]
    return f"{a}_{b}_{c}"

# ---- CA + leaf cert ----
def mk_ca():
    key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    sub=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,"fp-capture-ca")])
    now=datetime.datetime(2025,1,1)
    cert=(x509.CertificateBuilder().subject_name(sub).issuer_name(sub)
          .public_key(key.public_key()).serial_number(1)
          .not_valid_before(now).not_valid_after(datetime.datetime(2035,1,1))
          .add_extension(x509.BasicConstraints(ca=True,path_length=None),True)
          .sign(key,hashes.SHA256()))
    return key,cert
def mk_leaf(host,ca_key,ca_cert):
    key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
    sub=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,host)])
    try: san=x509.IPAddress(ipaddress.ip_address(host))
    except ValueError: san=x509.DNSName(host)
    now=datetime.datetime(2025,1,1)
    cert=(x509.CertificateBuilder().subject_name(sub).issuer_name(ca_cert.subject)
          .public_key(key.public_key()).serial_number(x509.random_serial_number())
          .not_valid_before(now).not_valid_after(datetime.datetime(2035,1,1))
          .add_extension(x509.SubjectAlternativeName([san]),False)
          .sign(ca_key,hashes.SHA256()))
    return key,cert

def read_connect(conn):
    buf=b""
    while b"\r\n\r\n" not in buf:
        c=conn.recv(4096)
        if not c: break
        buf+=c
    host=buf.split(b"\r\n",1)[0].split(b" ")[1].decode() if b" " in buf else "?:0"
    conn.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
    return host.split(":")[0], buf.decode('latin1','replace')

def main():
    port=int(sys.argv[1]) if len(sys.argv)>1 else 8888
    ca_out=sys.argv[2] if len(sys.argv)>2 else "/tmp/fpca.pem"
    ca_key,ca_cert=mk_ca()
    with open(ca_out,"wb") as f:
        f.write(ca_cert.public_bytes(serialization.Encoding.PEM))
    print(f"[CA written to {ca_out}; listening 127.0.0.1:{port}]",flush=True)
    s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    s.bind(("127.0.0.1",port));s.listen(5);s.settimeout(60)
    while True:
        try: conn,_=s.accept()
        except socket.timeout: print("[timeout]");return
        try:
            host,connreq=read_connect(conn)
            print("[CONNECT %s]"%host,flush=True)
            # peek ClientHello
            conn.setblocking(True)
            peek=b""
            for _ in range(20):
                peek=conn.recv(8192,socket.MSG_PEEK)
                if len(peek)>=5 and len(peek)>=5+u16(peek,3): break
            p=parse(peek[5:5+u16(peek,3)])
            j3s,j3=ja3(p)
            print("JA3 hash :",j3);print("JA4      :",ja4(p))
            print("ALPN offered:",p['alpns'],"SNI:",p['host'])
            # also surface UA from CONNECT request if present
            for ln in connreq.split("\r\n"):
                if ln.lower().startswith("user-agent:"): print("CONNECT-UA:",ln.split(":",1)[1].strip())
            # terminate TLS to read inner UA
            lk,lc=mk_leaf(host,ca_key,ca_cert)
            ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            with tempfile.NamedTemporaryFile("wb",delete=False,suffix=".pem") as cf:
                cf.write(lc.public_bytes(serialization.Encoding.PEM))
                cf.write(lk.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.TraditionalOpenSSL,serialization.NoEncryption()))
                cpath=cf.name
            ctx.load_cert_chain(cpath)
            try: ctx.set_alpn_protocols(["http/1.1"])
            except Exception: pass
            os.unlink(cpath)
            try:
                tconn=ctx.wrap_socket(conn,server_side=True)
            except ssl.SSLError as e:
                print("[TLS handshake rejected by client]",e);print("="*60,flush=True);conn.close();continue
            req=b""
            tconn.settimeout(8)
            try:
                while b"\r\n\r\n" not in req and len(req)<16384:
                    c=tconn.recv(4096)
                    if not c: break
                    req+=c
            except Exception: pass
            txt=req.decode('latin1','replace')
            line1=txt.split("\r\n",1)[0]
            print("REQUEST  :",line1[:120])
            ua=None
            for ln in txt.split("\r\n"):
                if ln.lower().startswith("user-agent:"): ua=ln.split(":",1)[1].strip()
            print("USER-AGENT:",ua if ua else "(none seen)")
            print("="*60,flush=True)
            try: tconn.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            except Exception: pass
            tconn.close();return
        except Exception as e:
            print("[err]",repr(e),flush=True)
            try: conn.close()
            except Exception: pass

if __name__=="__main__": main()
