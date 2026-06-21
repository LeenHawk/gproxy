#!/usr/bin/env python3
"""Passively capture and fingerprint (JA3 + JA4) a client's first TLS ClientHello.

Two modes:
  direct : listen on a port; the client connects straight to it (TLS endpoint).
  proxy  : act as an HTTP CONNECT proxy; the ClientHello is the first plaintext
           through the tunnel, so it is parsed before the handshake completes.
           Works regardless of OAuth vs API-key auth.

Usage:
  python3 capture_tls_fp.py 8888 proxy     # then point the client's *_proxy env at it
  python3 capture_tls_fp.py 8443           # direct mode

Capturing an agent CLI (override BOTH upper- and lower-case proxy vars, and
clear no_proxy so the destination is not bypassed):

  env HTTPS_PROXY=http://127.0.0.1:8888 https_proxy=http://127.0.0.1:8888 \\
      HTTP_PROXY=http://127.0.0.1:8888  http_proxy=http://127.0.0.1:8888 \\
      no_proxy= NO_PROXY= \\
    timeout 30 <the-cli> <one-shot-prompt-flags>
"""
import socket, struct, hashlib, sys

GREASE = {0x0a0a,0x1a1a,0x2a2a,0x3a3a,0x4a4a,0x5a5a,0x6a6a,0x7a7a,
          0x8a8a,0x9a9a,0xaaaa,0xbaba,0xcaca,0xdada,0xeaea,0xfafa}

def u16(b,i): return (b[i]<<8)|b[i+1]

def read_clienthello(conn):
    hdr = b""
    while len(hdr) < 5:
        c = conn.recv(5-len(hdr))
        if not c: raise EOFError("no data")
        hdr += c
    rectype, ver, rlen = hdr[0], u16(hdr,1), u16(hdr,3)
    body = b""
    while len(body) < rlen:
        c = conn.recv(rlen-len(body))
        if not c: break
        body += c
    return rectype, body

def parse(body):
    assert body[0]==0x01, "not ClientHello (type=%d)"%body[0]
    i = 4
    client_version = u16(body,i); i+=2
    i += 32  # random
    sid_len = body[i]; i+=1+sid_len
    cs_len = u16(body,i); i+=2
    ciphers=[u16(body,i+j) for j in range(0,cs_len,2)]
    i += cs_len
    comp_len = body[i]; i+=1+comp_len
    ext_total = u16(body,i); i+=2
    end = i+ext_total
    exts=[]; curves=[]; ecpf=[]; sni=False; alpns=[]; sigalgs=[]; sup_vers=[]
    while i < end:
        etype=u16(body,i); elen=u16(body,i+2); edata=body[i+4:i+4+elen]
        i += 4+elen
        exts.append(etype)
        if etype==0x000a:
            n=u16(edata,0)
            for j in range(0,n,2): curves.append(u16(edata,2+j))
        elif etype==0x000b:
            n=edata[0]
            for j in range(n): ecpf.append(edata[1+j])
        elif etype==0x0000:
            sni=True
        elif etype==0x0010:
            lst_len=u16(edata,0); k=2
            while k<2+lst_len:
                slen=edata[k]; alpns.append(edata[k+1:k+1+slen].decode('latin1')); k+=1+slen
        elif etype==0x000d:
            n=u16(edata,0)
            for j in range(0,n,2): sigalgs.append(u16(edata,2+j))
        elif etype==0x002b:
            n=edata[0]
            for j in range(0,n,2): sup_vers.append(u16(edata,1+j))
    return dict(client_version=client_version,ciphers=ciphers,exts=exts,
                curves=curves,ecpf=ecpf,sni=sni,alpns=alpns,sigalgs=sigalgs,
                sup_vers=sup_vers)

def ng(lst): return [x for x in lst if x not in GREASE]

def ja3(p):
    f=lambda xs:"-".join(str(x) for x in ng(xs))
    s=f"{p['client_version']},{f(p['ciphers'])},{f(p['exts'])},{f(p['curves'])},{f(p['ecpf'])}"
    return s, hashlib.md5(s.encode()).hexdigest()

VERMAP={0x0304:"13",0x0303:"12",0x0302:"11",0x0301:"10"}
def ja4(p, proto="t"):
    vers=ng(p['sup_vers']) or [p['client_version']]
    tv=VERMAP.get(max(vers),"00")
    snic = "d" if p['sni'] else "i"
    nc=ng(p['ciphers']); nce=ng(p['exts'])
    cc=min(len(nc),99); ec=min(len(nce),99)
    if p['alpns']:
        a=p['alpns'][0]; alpn=(a[0]+a[-1]) if len(a)>=2 else (a[0]+a[0]) if a else "00"
    else:
        alpn="00"
    a=f"{proto}{tv}{snic}{cc:02d}{ec:02d}{alpn}"
    cs=sorted("%04x"%c for c in nc)
    b=hashlib.sha256(",".join(cs).encode()).hexdigest()[:12] if cs else "000000000000"
    ce=sorted("%04x"%e for e in nce if e not in (0x0000,0x0010))
    sa=[("%04x"%s) for s in p['sigalgs']]
    cstr=",".join(ce)+"_"+",".join(sa)
    c=hashlib.sha256(cstr.encode()).hexdigest()[:12]
    return f"{a}_{b}_{c}", a, b, c

def read_connect(conn):
    buf=b""
    while b"\r\n\r\n" not in buf:
        c=conn.recv(4096)
        if not c: break
        buf+=c
    line=buf.split(b"\r\n",1)[0].decode('latin1')
    host=line.split(" ")[1] if " " in line else "?"
    conn.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
    return host

def main():
    port=int(sys.argv[1]) if len(sys.argv)>1 else 8443
    proxy = (len(sys.argv)>2 and sys.argv[2]=="proxy")
    s=socket.socket(socket.AF_INET,socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    s.bind(("127.0.0.1",port)); s.listen(5)
    s.settimeout(60)
    print(f"[listening on 127.0.0.1:{port} mode={'proxy' if proxy else 'direct'}]",flush=True)
    while True:
        try:
            conn,addr=s.accept()
        except socket.timeout:
            print("[timeout, no connection]"); return
        try:
            if proxy:
                host=read_connect(conn)
                print("[CONNECT %s]"%host,flush=True)
            rt,body=read_clienthello(conn)
            p=parse(body)
            j3,j3h=ja3(p); jfull,ja,jb,jc=ja4(p)
            print("="*60)
            print("JA3 string :",j3)
            print("JA3 hash   :",j3h)
            print("JA4        :",jfull)
            print("  JA4_a    :",ja)
            print("  JA4_b    :",jb,"(sorted ciphers)")
            print("  JA4_c    :",jc,"(exts+sigalgs)")
            print("-"*60)
            print("client_version: 0x%04x"%p['client_version'])
            print("supported_versions:",["0x%04x"%v for v in p['sup_vers']])
            print("ciphers (%d):"%len(ng(p['ciphers'])),["0x%04x"%c for c in ng(p['ciphers'])])
            print("extensions (%d):"%len(ng(p['exts'])),["0x%04x"%e for e in ng(p['exts'])])
            print("curves:",["0x%04x"%c for c in ng(p['curves'])])
            print("ec_point_formats:",p['ecpf'])
            print("ALPN:",p['alpns'])
            print("sig_algs:",["0x%04x"%x for x in p['sigalgs']])
            print("="*60,flush=True)
            conn.close()
            return
        except Exception as e:
            print("[parse error]",repr(e),flush=True)
            conn.close()

if __name__=="__main__":
    main()
