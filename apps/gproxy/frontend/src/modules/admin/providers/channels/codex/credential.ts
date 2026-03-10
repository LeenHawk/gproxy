import type { ChannelCredentialSchema } from "../../types";

export const CREDENTIAL_SCHEMA: ChannelCredentialSchema = {
  "channel": "codex",
  "kind": "builtin/codex",
  "wrapper": "Builtin",
  "builtinVariant": "Codex",
  "fields": [
    {
      "key": "access_token",
      "label": "access_token",
      "type": "string"
    },
    {
      "key": "refresh_token",
      "label": "refresh_token",
      "type": "string"
    },
    {
      "key": "id_token",
      "label": "id_token",
      "type": "string"
    },
    {
      "key": "priority_tier",
      "label": "priority_tier",
      "type": "optional_boolean"
    },
    {
      "key": "user_email",
      "label": "user_email",
      "type": "optional_string"
    },
    {
      "key": "account_id",
      "label": "account_id",
      "type": "string"
    },
    {
      "key": "expires_at",
      "label": "expires_at",
      "type": "integer"
    }
  ]
};
