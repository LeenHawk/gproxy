#!/usr/bin/env python3
"""Upload a signed HarmonyOS APP to AppGallery Connect without submitting it for review."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

API = "https://connect-api.cloud.huawei.com/api/publish"
AUDIENCE = "https://oauth-login.cloud.huawei.com/oauth2/v3/token"


def encode(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def token(account):
    """A developer-level service account signs its own PS256 bearer token."""
    now = int(time.time())
    header = {"kid": account["key_id"], "typ": "JWT", "alg": "PS256"}
    claims = {"aud": AUDIENCE, "iss": account["sub_account"], "exp": now + 3600, "iat": now}
    signed = ".".join(encode(json.dumps(part).encode()) for part in (header, claims))
    with tempfile.TemporaryDirectory(prefix="gproxy-agc-") as work:
        key = Path(work) / "key.pem"
        key.touch(mode=0o600)
        key.write_text(account["private_key"])
        signature = subprocess.run(
            ["openssl", "dgst", "-sha256", "-sigopt", "rsa_padding_mode:pss",
             "-sigopt", "rsa_pss_saltlen:32", "-sign", str(key)],
            input=signed.encode(), stdout=subprocess.PIPE, check=True).stdout
    return f"{signed}.{encode(signature)}"


def request(method, url, headers, body=None, timeout=600):
    call = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with urllib.request.urlopen(call, timeout=timeout) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"{method} {url.split('?')[0]}: HTTP {error.code} {error.reason} "
                           f"{error.read().decode(errors='replace')[:500]}") from None


def connect(method, path, bearer, query, body=None):
    headers = {"Authorization": f"Bearer {bearer}", "Content-Type": "application/json"}
    data = None if body is None else json.dumps(body).encode()
    result = json.loads(request(method, f"{API}/{path}?{urllib.parse.urlencode(query)}", headers, data, 60))
    if result.get("ret", {}).get("code") != 0:
        raise RuntimeError(f"{method} {path}: {result.get('ret')}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path, help="signed APP bundle")
    args = parser.parse_args()
    account = json.loads(os.environ.pop("AGC_SERVICE_ACCOUNT"))
    app = os.environ["AGC_APP_ID"]
    data = args.package.read_bytes()
    bearer = token(account)

    # Developers registered outside mainland China must say where the package goes.
    countries = connect("GET", "v3/app-info", bearer, {"appId": app})["appInfo"].get("publishCountry", "")
    upload = connect("GET", "v2/upload-url/for-obs", bearer, {
        "appId": app, "fileName": args.package.name,
        "sha256": hashlib.sha256(data).hexdigest(), "contentLength": len(data),
        "chineseMainlandFlag": int("CN" in countries.split(",")),
    })["urlInfo"]
    request(upload["method"], upload["url"], upload.get("headers", {}), data)
    package = connect("PUT", "v3/app-package-info", bearer, {"appId": app},
                      {"fileName": args.package.name, "objectId": upload["objectId"]})["packageId"]
    print(f"Uploaded {args.package.name} as package {package}; waiting for AppGallery Connect to parse it")

    # 0 = usable, 1 = still parsing, 2 = rejected.
    for _ in range(60):
        states = connect("GET", "v3/package/compile/status", bearer, {"appId": app, "pkgIds": package})["pkgStateList"]
        status = states[0]["successStatus"] if states else 1
        if status == 0:
            print("Package accepted. Select it and submit for review in AppGallery Connect.")
            return
        if status == 2:
            raise RuntimeError(f"AppGallery Connect rejected the package: {states[0]}")
        time.sleep(10)
    raise RuntimeError("AppGallery Connect did not finish parsing the package within 10 minutes")


if __name__ == "__main__":
    main()
