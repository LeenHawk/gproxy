import { createPrivateKey, createPublicKey } from "node:crypto"
import { readFileSync } from "node:fs"

const privateKey = createPrivateKey(Buffer.from(process.env.UPDATE_SIGNING_PRIVATE_KEY_B64 ?? "", "base64"))
const publicKey = createPublicKey(privateKey).export({ type: "spki", format: "der" }).subarray(-32)
const configured = Buffer.from(process.env.UPDATE_SIGNING_PUBLIC_KEY_B64 ?? "", "base64")
const compiled = Buffer.from(readFileSync(".cnb/update-public-key", "utf8").trim(), "base64")
if (!publicKey.equals(configured) || !publicKey.equals(compiled)) throw new Error("Update signing keys do not match the compiled trust root")
for (const name of ["ANDROID_SIGNING_KEYSTORE_B64", "ANDROID_SIGNING_KEYSTORE_PASSWORD", "ANDROID_SIGNING_KEY_ALIAS", "ANDROID_SIGNING_KEY_PASSWORD"]) {
  if (!process.env[name]) throw new Error(`${name} is missing`)
}
console.log("Update trust root and Android signing configuration verified")
