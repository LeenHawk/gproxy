
(() => {
  const g = globalThis;
  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  g.btoa = function btoa(s){ s=String(s); let out=''; for(let i=0;i<s.length;i+=3){ let c0=s.charCodeAt(i),c1=i+1<s.length?s.charCodeAt(i+1):0,c2=i+2<s.length?s.charCodeAt(i+2):0; if(c0>255||c1>255||c2>255) throw new Error('InvalidCharacter'); const n=(c0<<16)|(c1<<8)|c2; out+=B64[(n>>18)&63]+B64[(n>>12)&63]+(i+1<s.length?B64[(n>>6)&63]:'=')+(i+2<s.length?B64[n&63]:'='); } return out; };
  g.atob = function atob(s){ s=String(s).replace(/[ \t\n\f\r]/g,''); if(s.length%4===0&&s.endsWith('=')){ s=s.slice(0,-1); if(s.endsWith('=')) s=s.slice(0,-1);} let out='',buf=0,bits=0; for(const ch of s){ const v=B64.indexOf(ch); if(v<0) throw new Error('InvalidCharacter'); buf=(buf<<6)|v; bits+=6; if(bits>=8){ bits-=8; out+=String.fromCharCode((buf>>bits)&255);} } return out; };
  g.queueMicrotask = function queueMicrotask(cb){ Promise.resolve().then(()=>{ cb(); }); };
  g.structuredClone = function structuredClone(v){ const seen=new Map(); const go=x=>{ if(x===null||typeof x!=='object') return x; if(seen.has(x)) return seen.get(x); if(Array.isArray(x)){ const a=[]; seen.set(x,a); for(const e of x) a.push(go(e)); return a;} if(x instanceof Date) return new Date(x.getTime()); if(x instanceof Map){ const m=new Map(); seen.set(x,m); for(const [k,val] of x) m.set(go(k),go(val)); return m;} if(x instanceof Set){ const st=new Set(); seen.set(x,st); for(const e of x) st.add(go(e)); return st;} if(ArrayBuffer.isView(x)){ return new x.constructor(x);} if(x instanceof ArrayBuffer){ return x.slice(0);} const o={}; seen.set(x,o); for(const k of Object.keys(x)) o[k]=go(x[k]); return o; }; return go(v); };

  const abLen = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype,'byteLength').get;
  const taTag = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Int8Array.prototype),Symbol.toStringTag).get;
  const dvLen = Object.getOwnPropertyDescriptor(DataView.prototype,'byteLength').get;
  const mapSize = Object.getOwnPropertyDescriptor(Map.prototype,'size').get;
  const setSize = Object.getOwnPropertyDescriptor(Set.prototype,'size').get;
  const dateGet = Date.prototype.getTime;
  const reSrc = Object.getOwnPropertyDescriptor(RegExp.prototype,'source').get;
  const wmHas = WeakMap.prototype.has;
  const wsHas = WeakSet.prototype.has;
  const toS = Object.prototype.toString;
  const kindOf = (o) => {
    if (o===null || (typeof o!=='object' && typeof o!=='function')) return '';
    if (Array.isArray(o)) return 'Array';
    try { abLen.call(o); return 'ArrayBuffer'; } catch {}
    try { if (taTag.call(o)!==undefined) return 'TypedArray'; } catch {}
    try { dvLen.call(o); return 'DataView'; } catch {}
    try { wmHas.call(o,{}); return 'WeakMap'; } catch {}
    try { mapSize.call(o); return 'Map'; } catch {}
    try { setSize.call(o); return 'Set'; } catch {}
    try { dateGet.call(o); return 'Date'; } catch {}
    try { reSrc.call(o); return 'RegExp'; } catch {}
    if (o instanceof Error) return 'Error';
    if (typeof Promise==='function' && o instanceof Promise) return 'Promise';
    const tag = toS.call(o);
    if (tag==='[object Arguments]') return 'Arguments';
    if (tag==='[object String]') return 'String';
    if (tag==='[object Boolean]') return 'Boolean';
    if (tag==='[object Number]') return 'Number';
    if (tag==='[object BigInt]') return 'BigInt';
    try { wsHas.call(o,{}); return 'WeakSet'; } catch {}
    if (typeof o==='function') return 'Function';
    const name = (o.constructor && o.constructor.name) || '';
    if (name.indexOf('Iterator')>=0 || name.indexOf('Generator')>=0) return name;
    return 'Object';
  };

  let throwLoc = ['', 0, 0];
  const parseStack = (e) => { try { const line=String(e&&e.stack||'').split('\n').find(l=>l.indexOf(':')>=0)||''; const m=line.match(/\(?([^():]+):(\d+):(\d+)\)?\s*$/); if(m) throwLoc=[m[1],+m[2],+m[3]]; } catch {} };

  const H = g.__host;
  const natives = {
    now: () => H.now(),
    schedule: (d, id) => H.schedule(d, id),
    digest: (name, data) => H.digest(name, data),
    randomUUID: () => H.randomUUID(),
    randomBytes: (n) => H.randomBytes(n),
    encodingName: (label) => H.encodingName(label),
    decodeText: (name, data, fatal) => H.decodeText(name, data, fatal),
    parseURL: (t, b) => H.parseURL(t, b),
    updateURL: (h, f, v) => H.updateURL(h, f, v),
    incumbent: () => g,
    kindOf,
    throwLocation: () => throwLoc,
    callListener: (fn, thisArg, ...args) => { try { return Reflect.apply(fn, thisArg, args); } catch (e) { parseStack(e); throw e; } },
    promiseResult: (_p) => undefined,
    lazyGlobals: (resolve, enumerate, names) => { g.__lazyResolve = resolve; g.__lazyEnumerate = enumerate; },
    loadImage: (address, id) => H.loadImage(address, id),
  };
  __IFRAME_LINE__
  g.__natives = natives;
})();
