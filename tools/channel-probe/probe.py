#!/usr/bin/env python3
"""gproxy 单渠道端点抓取工具(scoped 模式)。

对一个正在运行的 gproxy 实例,把一个渠道(provider)接受的所有入站端点 × 线格式
各打一遍,把「实际发出的请求」与「响应」一并存成可读 JSON。纯抓取,不做任何断言。

URL 形态:{base_url}/{provider}/v1/...  (scoped:绕过 route 直连指定渠道)

占位符(运行时替换):
  {{MODEL}}  -> --model            (内容生成 / 计数 / 压缩 / models)
  {{EMBED}}  -> --embedding-model  (嵌入;未设时回落 --model)
  {{IMAGE}}  -> --image-model      (生图;未设时回落 --model)

key 按 spec 放对应鉴权头(也是 gproxy classify 区分 openai/claude 的依据):
  openai -> Authorization: Bearer   claude -> x-api-key   gemini -> x-goog-api-key

用法:
  python3 probe.py --provider codex --model gpt-5.4 --key gp-xxx
  python3 probe.py --config probe.json --only content,count
"""

import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

# ── 20 条请求(改请求就改这里)────────────────────────────────────────────────
# cat:   仅供 --only 过滤 / 阅读分类,程序语义无关
# spec:  openai | claude | gemini —— 决定 key 放哪个头(claude 另带 anthropic-version)
# path:  provider 相对路径,运行时前缀 /{provider};{{MODEL}} 会被替换
# body:  None = 无正文(GET);否则按 json 发送
HELLO = "Reply with a single word: hello."
COUNT = "Count the tokens in this sentence please."

REQUESTS = [
    # ── 内容生成(8):4 种入站线格式 × 流/非流 ──────────────────────────────
    {"name": "openai-chat", "cat": "content", "spec": "openai", "method": "POST",
     "path": "/v1/chat/completions", "stream": False,
     "body": {"model": "{{MODEL}}", "max_tokens": 32,
              "messages": [{"role": "user", "content": HELLO}]}},
    {"name": "openai-chat-stream", "cat": "content", "spec": "openai", "method": "POST",
     "path": "/v1/chat/completions", "stream": True,
     "body": {"model": "{{MODEL}}", "max_tokens": 32, "stream": True,
              "messages": [{"role": "user", "content": HELLO}]}},
    {"name": "openai-responses", "cat": "content", "spec": "openai", "method": "POST",
     "path": "/v1/responses", "stream": False,
     "body": {"model": "{{MODEL}}", "max_output_tokens": 32, "input": HELLO}},
    {"name": "openai-responses-stream", "cat": "content", "spec": "openai", "method": "POST",
     "path": "/v1/responses", "stream": True,
     "body": {"model": "{{MODEL}}", "max_output_tokens": 32, "stream": True, "input": HELLO}},
    {"name": "claude-messages", "cat": "content", "spec": "claude", "method": "POST",
     "path": "/v1/messages", "stream": False,
     "body": {"model": "{{MODEL}}", "max_tokens": 32,
              "messages": [{"role": "user", "content": HELLO}]}},
    {"name": "claude-messages-stream", "cat": "content", "spec": "claude", "method": "POST",
     "path": "/v1/messages", "stream": True,
     "body": {"model": "{{MODEL}}", "max_tokens": 32, "stream": True,
              "messages": [{"role": "user", "content": HELLO}]}},
    {"name": "gemini-generate", "cat": "content", "spec": "gemini", "method": "POST",
     "path": "/v1beta/models/{{MODEL}}:generateContent", "stream": False,
     "body": {"contents": [{"role": "user", "parts": [{"text": HELLO}]}]}},
    {"name": "gemini-stream", "cat": "content", "spec": "gemini", "method": "POST",
     "path": "/v1beta/models/{{MODEL}}:streamGenerateContent?alt=sse", "stream": True,
     "body": {"contents": [{"role": "user", "parts": [{"text": HELLO}]}]}},

    # ── 模型(6):list / get × openai / claude / gemini ─────────────────────
    {"name": "openai-list", "cat": "models", "spec": "openai", "method": "GET",
     "path": "/v1/models", "stream": False, "body": None},
    {"name": "claude-list", "cat": "models", "spec": "claude", "method": "GET",
     "path": "/v1/models", "stream": False, "body": None},
    {"name": "gemini-list", "cat": "models", "spec": "gemini", "method": "GET",
     "path": "/v1beta/models", "stream": False, "body": None},
    {"name": "openai-get", "cat": "models", "spec": "openai", "method": "GET",
     "path": "/v1/models/{{MODEL}}", "stream": False, "body": None},
    {"name": "claude-get", "cat": "models", "spec": "claude", "method": "GET",
     "path": "/v1/models/{{MODEL}}", "stream": False, "body": None},
    {"name": "gemini-get", "cat": "models", "spec": "gemini", "method": "GET",
     "path": "/v1beta/models/{{MODEL}}", "stream": False, "body": None},

    # ── 计数(3):claude / openai / gemini ─────────────────────────────────
    {"name": "claude-count-tokens", "cat": "count", "spec": "claude", "method": "POST",
     "path": "/v1/messages/count_tokens", "stream": False,
     "body": {"model": "{{MODEL}}", "messages": [{"role": "user", "content": COUNT}]}},
    {"name": "openai-input-tokens", "cat": "count", "spec": "openai", "method": "POST",
     "path": "/v1/responses/input_tokens", "stream": False,
     "body": {"model": "{{MODEL}}", "input": COUNT}},
    {"name": "gemini-count-tokens", "cat": "count", "spec": "gemini", "method": "POST",
     "path": "/v1beta/models/{{MODEL}}:countTokens", "stream": False,
     "body": {"contents": [{"role": "user", "parts": [{"text": COUNT}]}]}},

    # ── 嵌入(2):openai 兼容 + gemini 原生 ──────────────────────────────────
    {"name": "openai-embeddings", "cat": "embedding", "spec": "openai", "method": "POST",
     "path": "/v1/embeddings", "stream": False,
     "body": {"model": "{{EMBED}}", "input": "hello world"}},
    {"name": "gemini-embeddings", "cat": "embedding", "spec": "gemini", "method": "POST",
     "path": "/v1beta/models/{{EMBED}}:embedContent", "stream": False,
     "body": {"model": "models/{{EMBED}}", "content": {"parts": [{"text": "hello world"}]}}},

    # ── 生图(1)──────────────────────────────────────────────────────────────
    {"name": "openai-image-generations", "cat": "image", "spec": "openai", "method": "POST",
     "path": "/v1/images/generations", "stream": False,
     "body": {"model": "{{IMAGE}}", "prompt": "a small red cube on white background",
              "n": 1, "size": "1024x1024"}},

    # ── 压缩(1):compact;classify 当前无此入站路由,预期 404,如实记录 ────────
    {"name": "openai-compact", "cat": "compact", "spec": "openai", "method": "POST",
     "path": "/v1/responses/compact", "stream": False,
     "body": {"model": "{{MODEL}}", "input": "Summarize the prior conversation."}},
]


# ── 运行 ──────────────────────────────────────────────────────────────────────
def subst(text, models):
    return (text.replace("{{MODEL}}", models["model"])
                .replace("{{EMBED}}", models["embed"])
                .replace("{{IMAGE}}", models["image"]))


def headers_for(spec, key, has_body, stream):
    h = {}
    if spec == "openai":
        h["Authorization"] = f"Bearer {key}"
    elif spec == "claude":
        h["x-api-key"] = key
        h["anthropic-version"] = "2023-06-01"
    elif spec == "gemini":
        h["x-goog-api-key"] = key
    if has_body:
        h["Content-Type"] = "application/json"
    if stream:
        h["Accept"] = "text/event-stream"
    return h


def redact(headers):
    secret = {"authorization", "x-api-key", "x-goog-api-key"}
    return {k: ("***" if k.lower() in secret else v) for k, v in headers.items()}


def try_json(text):
    try:
        return json.loads(text)
    except (ValueError, TypeError):
        return text


def read_sse(resp):
    """把 SSE 流解码成事件数组(只取 data: 行,[DONE] 原样保留)。"""
    events, buf = [], []
    for raw in resp:
        line = raw.decode("utf-8", "replace").rstrip("\r\n")
        if line == "":
            if buf:
                payload = "\n".join(buf)
                buf = []
                events.append("[DONE]" if payload == "[DONE]" else try_json(payload))
            continue
        if line.startswith(":"):
            continue  # 注释/心跳
        if line.startswith("data:"):
            buf.append(line[5:].lstrip())
    if buf:
        payload = "\n".join(buf)
        events.append("[DONE]" if payload == "[DONE]" else try_json(payload))
    return events


def capture(resp, t0):
    elapsed = int((time.monotonic() - t0) * 1000)
    ct = resp.headers.get("Content-Type", "")
    out = {"status": resp.getcode(), "headers": dict(resp.headers.items()),
           "elapsed_ms": elapsed}
    if "text/event-stream" in ct:
        out["events"] = read_sse(resp)
    else:
        out["body"] = try_json(resp.read().decode("utf-8", "replace"))
    return out


def send(method, url, headers, body, timeout):
    req = urllib.request.Request(url, data=body, method=method, headers=headers)
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return capture(resp, t0)
    except urllib.error.HTTPError as e:        # 非 2xx:HTTPError 也能当响应读
        return capture(e, t0)
    except Exception as e:                     # 传输/超时失败
        return {"status": None, "error": repr(e),
                "elapsed_ms": int((time.monotonic() - t0) * 1000)}


def run_one(r, base, provider, key, models, timeout):
    path = subst(r["path"], models)
    url = f"{base.rstrip('/')}/{provider}{path}"
    body_bytes, body_view = None, None
    if r["body"] is not None:
        body_str = subst(json.dumps(r["body"]), models)
        body_bytes = body_str.encode("utf-8")
        body_view = json.loads(body_str)
    headers = headers_for(r["spec"], key, body_bytes is not None, r["stream"])
    response = send(r["method"], url, headers, body_bytes, timeout)
    return {
        "request": {"method": r["method"], "url": url,
                    "headers": redact(headers), "body": body_view},
        "response": response,
    }


def main(argv=None):
    p = argparse.ArgumentParser(description="gproxy 单渠道端点抓取(scoped)")
    p.add_argument("--config", help="JSON 配置文件(字段同下,CLI 旗标覆盖之)")
    p.add_argument("--base-url")
    p.add_argument("--provider")
    p.add_argument("--key")
    p.add_argument("--model")
    p.add_argument("--embedding-model")
    p.add_argument("--image-model")
    p.add_argument("--only", help="逗号分隔:按 cat(content/models/count/embedding/"
                                  "image/compact)或 name 过滤")
    p.add_argument("--out", default="./probe-out", help="输出目录(默认 ./probe-out)")
    p.add_argument("--timeout", type=float, default=120.0)
    p.add_argument("--list", action="store_true", help="只列出请求清单,不发送")
    args = p.parse_args(argv)

    if args.list:
        for r in REQUESTS:
            print(f"{r['cat']:9} {r['name']:26} {r['method']:4} {r['path']}")
        return 0

    cfg = {}
    if args.config:
        cfg = json.loads(Path(args.config).read_text(encoding="utf-8"))

    def pick(flag, key, default=None):
        return flag if flag is not None else cfg.get(key, default)

    base = pick(args.base_url, "base_url", "http://127.0.0.1:8787")
    provider = pick(args.provider, "provider")
    key = pick(args.key, "api_key", "")
    model = pick(args.model, "model")
    if not provider or not model:
        p.error("缺少 --provider 或 --model(也可经 --config 提供)")
    models = {
        "model": model,
        "embed": pick(args.embedding_model, "embedding_model", model),
        "image": pick(args.image_model, "image_model", model),
    }

    selected = REQUESTS
    if args.only:
        wanted = {s.strip() for s in args.only.split(",") if s.strip()}
        selected = [r for r in REQUESTS if r["cat"] in wanted or r["name"] in wanted]
        if not selected:
            p.error(f"--only {args.only} 没匹配到任何请求")

    runid = datetime.now().strftime("%Y%m%d-%H%M%S")
    out_dir = Path(args.out) / f"{provider}-{runid}"
    out_dir.mkdir(parents=True, exist_ok=True)
    print(f"provider={provider} model={model} base={base}")
    print(f"→ {len(selected)} 条,输出 {out_dir}\n")

    for r in selected:
        record = run_one(r, base, provider, key, models, args.timeout)
        (out_dir / f"{r['name']}.json").write_text(
            json.dumps(record, ensure_ascii=False, indent=2), encoding="utf-8")
        status = record["response"].get("status")
        nev = record["response"].get("events")
        extra = f" events={len(nev)}" if nev is not None else ""
        print(f"  {r['name']:26} {str(status):>4}{extra}")

    print(f"\n完成:{len(selected)} 个文件 → {out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
