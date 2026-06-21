#!/usr/bin/env bash
# Forwarding-MITM capture of each agent CLI's MODEL-PATH UA + TLS(JA4) + HTTP/2
# fingerprint. Relays to the REAL upstream so auth/token pre-checks succeed and
# the client proceeds to its model call; passively sniffs the client->upstream
# plaintext and mirrors the client's ALPN. Safe to re-run.
#
# Requires the CLIs already logged in. For claude direct-to-anthropic it strips
# the gproxy ANTHROPIC_BASE_URL from ~/.claude/settings.json and restores on exit.
set -u
cd /home/linhuan/gproxy/v2

PORT=8889
CA=/tmp/fpca.pem
COMBINED=/tmp/fpca_combined.pem      # system roots + our fake CA (so BOTH direct and MITM'd TLS verify)
LOG=/tmp/fwd_capture.log
SETTINGS="$HOME/.claude/settings.json"
export UPSTREAM_PROXY="${https_proxy:-${HTTPS_PROXY:-}}"   # chain egress through the box's real proxy
echo "== egress via: ${UPSTREAM_PROXY:-direct} =="

# 1) strip gproxy base url from claude settings.json so claude hits api.anthropic.com directly
if [ -f "$SETTINGS" ]; then
  python3 - "$SETTINGS" <<'PY'
import json,sys,shutil
p=sys.argv[1]; shutil.copy(p,p+".fpbak")
d=json.load(open(p)); env=d.get("env",{}) or {}
rm={k:env.pop(k) for k in list(env) if "ANTHROPIC_BASE_URL" in k or "gproxy" in str(env.get(k,"")).lower()}
d["env"]=env; json.dump(d,open(p,"w"),indent=2); print("settings.json stripped:",rm)
PY
fi
restore(){ [ -f "$SETTINGS.fpbak" ] && mv -f "$SETTINGS.fpbak" "$SETTINGS" && echo "settings.json restored"; }
trap restore EXIT

# 2) start forwarding MITM (its own env verifies upstream with REAL CAs)
env -u SSL_CERT_FILE -u NODE_EXTRA_CA_CERTS UPSTREAM_PROXY="$UPSTREAM_PROXY" \
  python3 scripts/capture_fwd_mitm.py "$PORT" "$CA" >"$LOG" 2>&1 &
MITM=$!
sleep 2
grep -q "fwd-MITM" "$LOG" || { echo "MITM failed to start:"; cat "$LOG"; kill $MITM 2>/dev/null; exit 1; }
cat /etc/ssl/certs/ca-certificates.crt "$CA" > "$COMBINED"   # combined bundle AFTER the MITM writes its CA

# 3) run a CLI through the MITM. KEEP the original no_proxy; trust fake CA additively
#    (NODE_EXTRA_CA_CERTS=fake) + combined bundle for OpenSSL paths (SSL_CERT_FILE=combined),
#    so direct (non-MITM'd) calls still verify real certs.
runcli(){
  local name="$1"; shift
  echo "---- $name ----"
  env HTTPS_PROXY=http://127.0.0.1:$PORT https_proxy=http://127.0.0.1:$PORT \
      HTTP_PROXY=http://127.0.0.1:$PORT  http_proxy=http://127.0.0.1:$PORT \
      NODE_EXTRA_CA_CERTS=$CA SSL_CERT_FILE=$COMBINED REQUESTS_CA_BUNDLE=$COMBINED \
      "$@" </dev/null >/tmp/fwd_cli_$name.log 2>&1
}

GEMINI=/home/linhuan/.nvm/versions/node/v22.20.0/bin/gemini
COPILOT=/home/linhuan/.nvm/versions/node/v22.20.0/bin/copilot
CLAUDE=/home/linhuan/.local/bin/claude
GHT="$(gh auth token 2>/dev/null)"   # lets copilot skip its proxy-bypassing OAuth validation

runcli gemini  timeout 60 "$GEMINI"  -p "say hi in one word"
runcli copilot env GH_TOKEN="$GHT" COPILOT_GITHUB_TOKEN="$GHT" timeout 60 "$COPILOT" -p "say hi" --allow-all-tools
runcli claude  env ANTHROPIC_BASE_URL=https://api.anthropic.com timeout 60 "$CLAUDE" -p "say hi in one word" --max-turns 1

# 4) stop + report
sleep 1; kill $MITM 2>/dev/null; wait $MITM 2>/dev/null
echo; echo "============ MODEL-PATH FINGERPRINTS (FP|host|ja4|proto|akamai|path|UA) ============"
grep '^FP|' "$LOG" | sort -u
echo "===================================================================================="
echo "(full MITM log: $LOG ; per-cli logs: /tmp/fwd_cli_*.log)"
