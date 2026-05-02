pub mod claude_code;
pub mod claude_compat;
pub mod claude_family;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod gemini;
pub mod hermes;
pub mod openclaw;
pub mod opencode;

use crate::provider::AgentProvider;
use claude_code::ClaudeCodeProvider;
use claude_family::{
    CODEBUDDY_SPEC, ClaudeFamilyProvider, QODER_SPEC, QODERWORK_SPEC, QWEN_CODE_SPEC,
    WORKBUDDY_SPEC,
};
use codex::CodexProvider;
use copilot::CopilotProvider;
use cursor::CursorProvider;
use gemini::GeminiProvider;
use hermes::HermesProvider;
use openclaw::OpenClawProvider;
use opencode::OpenCodeProvider;
use std::path::Path;

pub fn builtin_providers() -> Vec<Box<dyn AgentProvider>> {
    vec![
        Box::new(ClaudeCodeProvider::new()),
        Box::new(CodexProvider::new()),
        Box::new(GeminiProvider::new()),
        Box::new(CursorProvider::new()),
        Box::new(CopilotProvider::new()),
        Box::new(HermesProvider::new()),
        Box::new(OpenClawProvider::new()),
        Box::new(OpenCodeProvider::new()),
        Box::new(ClaudeFamilyProvider::from_spec(&QWEN_CODE_SPEC)),
        Box::new(ClaudeFamilyProvider::from_spec(&QODER_SPEC)),
        Box::new(ClaudeFamilyProvider::from_spec(&QODERWORK_SPEC)),
        Box::new(ClaudeFamilyProvider::from_spec(&CODEBUDDY_SPEC)),
        Box::new(ClaudeFamilyProvider::from_spec(&WORKBUDDY_SPEC)),
    ]
}

/// Shell-quote a path for embedding in a hook command string.
pub(crate) fn shell_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    {
        if raw.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '\\' | '/' | '-' | '_' | '.' | ':')
        }) {
            return raw.into_owned();
        }

        return format!("\"{}\"", raw.replace('"', "\\\""));
    }

    #[cfg(not(windows))]
    {
        if raw
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.' | ':'))
        {
            return raw.into_owned();
        }

        format!("'{}'", raw.replace('\'', "'\"'\"'"))
    }
}

#[cfg(windows)]
pub(crate) fn transport_flag() -> &'static str {
    "--pipe"
}

#[cfg(not(windows))]
pub(crate) fn transport_flag() -> &'static str {
    "--socket"
}
