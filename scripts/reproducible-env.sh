#!/usr/bin/env bash
# Source this in a release entrypoint; derive time from source, never wall time.
gproxy_repro_env="$(python3 "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.py")" || return
eval "$gproxy_repro_env"
unset gproxy_repro_env
