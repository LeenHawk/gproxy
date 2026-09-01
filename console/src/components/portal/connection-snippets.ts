export type ConnectionMethod = "curl" | "openai" | "claude" | "gemini" | "codex" | "claudeCode"

export type ConnectionSnippet = {
  method: ConnectionMethod
  display: string
  copy: string
}

export const connectionSource: Record<ConnectionMethod, string> = {
  curl: "openai_chat",
  openai: "openai_chat",
  claude: "claude_messages",
  gemini: "gemini_generate_content",
  codex: "openai_responses",
  claudeCode: "claude_messages",
}

type Values = {
  origin: string
  model: string
  key: string
  keyPlaceholder: string
  prompt: string
}

const pythonString = (value: string) => JSON.stringify(value)
const tomlString = (value: string) => JSON.stringify(value)
const shellString = (value: string) => `'${value.replaceAll("'", `'"'"'`)}'`

export function connectionSnippets(values: Values): Array<ConnectionSnippet> {
  const display = snippets(values, values.keyPlaceholder)
  const copy = snippets(values, values.key)
  return display.map((snippet, index) => ({ ...snippet, copy: copy[index].display }))
}

function snippets(values: Values, key: string): Array<Omit<ConnectionSnippet, "copy">> {
  const openAiBase = `${values.origin}/v1`
  const chatBody = JSON.stringify({
    model: values.model,
    messages: [{ role: "user", content: values.prompt }],
  })
  return [
    {
      method: "curl",
      display: [
        `curl ${shellString(`${values.origin}/v1/chat/completions`)} \\`,
        `  -H ${shellString(`Authorization: Bearer ${key}`)} \\`,
        `  -H ${shellString("Content-Type: application/json")} \\`,
        `  -d ${shellString(chatBody)}`,
      ].join("\n"),
    },
    {
      method: "openai",
      display: [
        "from openai import OpenAI",
        "",
        `client = OpenAI(api_key=${pythonString(key)}, base_url=${pythonString(openAiBase)})`,
        "response = client.chat.completions.create(",
        `    model=${pythonString(values.model)},`,
        `    messages=[{"role": "user", "content": ${pythonString(values.prompt)}}],`,
        ")",
        "print(response.choices[0].message.content)",
      ].join("\n"),
    },
    {
      method: "claude",
      display: [
        "from anthropic import Anthropic",
        "",
        `client = Anthropic(api_key=${pythonString(key)}, base_url=${pythonString(values.origin)})`,
        "message = client.messages.create(",
        `    model=${pythonString(values.model)},`,
        "    max_tokens=1024,",
        `    messages=[{"role": "user", "content": ${pythonString(values.prompt)}}],`,
        ")",
        "print(message.content[0].text)",
      ].join("\n"),
    },
    {
      method: "gemini",
      display: [
        "from google import genai",
        "from google.genai import types",
        "",
        "client = genai.Client(",
        `    api_key=${pythonString(key)},`,
        `    http_options=types.HttpOptions(base_url=${pythonString(values.origin)}),`,
        ")",
        "response = client.models.generate_content(",
        `    model=${pythonString(values.model)},`,
        `    contents=${pythonString(values.prompt)},`,
        ")",
        "print(response.text)",
      ].join("\n"),
    },
    {
      method: "codex",
      display: [
        "# ~/.codex/config.toml",
        `model = ${tomlString(values.model)}`,
        'model_provider = "openai"',
        `openai_base_url = ${tomlString(`${values.origin}/codex/backend-api/codex`)}`,
        `chatgpt_base_url = ${tomlString(`${values.origin}/codex/backend-api`)}`,
        "",
        `export CODEX_REFRESH_TOKEN_URL_OVERRIDE=${shellString(`${values.origin}/codex/oauth/token`)}`,
        `export CODEX_REVOKE_TOKEN_URL_OVERRIDE=${shellString(`${values.origin}/codex/oauth/revoke`)}`,
        `codex login --device-auth --experimental_issuer ${shellString(`${values.origin}/codex`)} --experimental_client-id app_EMoamEEZ73f0CkXaXp7hrann`,
      ].join("\n"),
    },
    {
      method: "claudeCode",
      display: [
        `export ANTHROPIC_BASE_URL=${shellString(values.origin)}`,
        `export CLAUDE_CODE_OAUTH_TOKEN=${shellString(key)}`,
        `claude --model ${shellString(values.model)}`,
      ].join("\n"),
    },
  ]
}
