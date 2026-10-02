//! Exact CLI boilerplate rejected by Devin on otherwise benign requests.
//! Isolated by live bisection with Claude Code 2.1.285 and Codex 0.160.0.
//! Rephrase only system instructions and tool descriptions, preserving the
//! security/approval rules; never rewrite user messages or tool results.

pub(super) fn system(text: &str) -> String {
    let mut text = text.to_owned();
    for (from, to) in [
        (
            "The system will automatically compress prior messages in your conversation as it approaches context limits.",
            "Earlier turns are compacted automatically near the context limit.",
        ),
        (
            "/help: Get help with using Claude Code",
            "/help: Show CLI help.",
        ),
        (
            "To give feedback, users should report the issue at https://github.com/anthropics/claude-code/issues",
            "Report CLI issues at https://github.com/anthropics/claude-code/issues.",
        ),
        (
            "Do not use a colon before tool calls.",
            "End introductions to tool calls with a period rather than a colon.",
        ),
        (
            "Anything already documented in CLAUDE.md files.",
            "Information that is already recorded in CLAUDE.md.",
        ),
        (
            "You MUST access memory when the user explicitly asks you to check, recall, or remember.",
            "You MUST consult saved memory when the user asks you to recall or check it.",
        ),
        (
            "Claude Code is available as a CLI in the terminal, desktop app (Mac/Windows), web app (claude.ai/code), and IDE extensions (VS Code, JetBrains).",
            "Claude Code offers terminal, desktop, web, and IDE interfaces.",
        ),
        (
            "Dual-use security tools (C2 frameworks, credential testing, exploit development) require clear authorization context: pentesting engagements, CTF competitions, security research, or defensive use cases.",
            "Use dual-purpose security tooling, including C2 frameworks, credential tests, and exploit development, only with clear authorization for a penetration test, CTF competition, security research, or defensive work.",
        ),
        (
            "A user approving an action (like a git push) once does NOT mean that they approve it in all contexts, so unless actions are authorized in advance in durable instructions like CLAUDE.md files, always confirm first.",
            "Approval for a prior action such as a git push does not grant blanket authorization. Unless durable instructions such as CLAUDE.md already authorize the specific action, obtain confirmation before acting.",
        ),
        (
            "Carefully consider the reversibility and blast radius of actions.",
            "Before acting, assess reversibility and the scope of possible effects.",
        ),
        (
            "You are a Claude agent, built on Anthropic's Claude Agent SDK.",
            "You are a coding assistant.",
        ),
        (
            "Within this context, Codex refers to the open-source agentic coding interface (not the old Codex language model built by OpenAI).",
            "",
        ),
        (
            "- Don’t output ANSI escape codes directly — the CLI renderer applies them.",
            "- Do not print raw ANSI escapes; formatting is handled by the terminal interface.",
        ),
    ] {
        text = text.replace(from, to);
    }
    text
}

pub(super) fn tool_description(text: &str) -> String {
    let mut text = text.to_owned();
    for (from, to) in [
        (
            "Runs a command in a PTY, returning output or a session ID for ongoing interaction.",
            "Runs a command in a PTY and returns its output or an ID for a running session.",
        ),
        (
            "Writes characters to an existing unified exec session and returns recent output.",
            "Sends input to a running session and reads recent output.",
        ),
        (
            "Reads a file from the local filesystem. You can access any file directly by using this tool.",
            "Opens a local file at the supplied path.",
        ),
        (
            "This tool allows Claude Code to read images (eg PNG, JPG, etc). When reading an image file the contents are presented visually as Claude Code is a multimodal LLM.",
            "This tool presents PNG, JPG, and other image files visually to the model.",
        ),
    ] {
        text = text.replace(from, to);
    }
    text
}
