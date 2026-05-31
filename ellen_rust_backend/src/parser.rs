//! Parse LLM responses to extract motion/expression tags and produce clean text.
//!
//! The LLM is instructed to prefix every response with tag pairs like:
//! ```text
//! [motion:idle][exp:lazy] おはようございます、ご主人様。
//! ```
//!
//! This module parses those tags, validates them against whitelists, and strips
//! them to obtain human-readable text.

use regex::Regex;
use std::sync::OnceLock;

/// Whitelist of valid motion identifiers.
const VALID_MOTIONS: &[&str] = &[
    "idle",
    "idle2",
    "lazy_stretch",
    "alert",
    "shy_fidget",
    "hangry_sway",
];

/// Whitelist of valid expression identifiers.
const VALID_EXPRESSIONS: &[&str] = &["lazy", "maid", "predator", "hangry", "shy", "surprised", "happy"];

/// Default motion when the parsed value is missing or invalid.
const DEFAULT_MOTION: &str = "idle";

/// Default expression when the parsed value is missing or invalid.
const DEFAULT_EXPRESSION: &str = "lazy";

/// Result of parsing a raw LLM response string.
#[derive(Debug, Clone, Default)]
pub struct ParsedResponse {
    /// Extracted motion ID (validated against whitelist).
    pub motion_id: String,
    /// Extracted expression ID (validated against whitelist).
    pub expression_id: String,
    /// The original text with all `[motion:…]` and `[exp:…]` tags removed.
    pub clean_text: String,
    /// The original, unmodified response text.
    pub raw_text: String,
}

/// Returns the compiled regex for matching `[motion:ID]` tags.
fn motion_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[motion:([a-zA-Z0-9_]+)\]").unwrap())
}

/// Returns the compiled regex for matching `[exp:ID]` tags.
fn exp_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[exp:([a-zA-Z0-9_]+)\]").unwrap())
}

/// Returns the compiled regex for stripping all motion/expression tags.
fn strip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[(motion|exp):[a-zA-Z0-9_]+\]").unwrap())
}

/// Validates a motion ID against the whitelist.
fn validate_motion(id: &str) -> &str {
    if VALID_MOTIONS.contains(&id) {
        id
    } else {
        DEFAULT_MOTION
    }
}

/// Validates an expression ID against the whitelist.
fn validate_expression(id: &str) -> &str {
    if VALID_EXPRESSIONS.contains(&id) {
        id
    } else {
        DEFAULT_EXPRESSION
    }
}

/// Parse a raw LLM response string.
///
/// Extracts `[motion:…]` and `[exp:…]` tags, validates them against the
/// whitelist of known values, strips the tags to produce clean text, and
/// stores the original raw text for reference.
///
/// # Examples
///
/// ```no_run
/// use ellen_rust_backend::parser::parse_llm_response;
///
/// let parsed = parse_llm_response("[motion:idle][exp:lazy] おはよう");
/// assert_eq!(parsed.motion_id, "idle");
/// assert_eq!(parsed.expression_id, "lazy");
/// assert_eq!(parsed.clean_text.trim(), "おはよう");
/// ```
pub fn parse_llm_response(raw: &str) -> ParsedResponse {
    let motion_id = motion_regex()
        .captures(raw)
        .and_then(|caps| caps.get(1))
        .map(|m| validate_motion(m.as_str()).to_string())
        .unwrap_or_else(|| DEFAULT_MOTION.to_string());

    let expression_id = exp_regex()
        .captures(raw)
        .and_then(|caps| caps.get(1))
        .map(|m| validate_expression(m.as_str()).to_string())
        .unwrap_or_else(|| DEFAULT_EXPRESSION.to_string());

    let clean_text = strip_regex().replace_all(raw, "").to_string();

    ParsedResponse {
        motion_id,
        expression_id,
        clean_text,
        raw_text: raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let raw = "[motion:idle][exp:lazy] おはよう、ご主人様。";
        let parsed = parse_llm_response(raw);
        assert_eq!(parsed.motion_id, "idle");
        assert_eq!(parsed.expression_id, "lazy");
        assert_eq!(parsed.clean_text.trim(), "おはよう、ご主人様。");
        assert_eq!(parsed.raw_text, raw);
    }

    #[test]
    fn test_parse_invalid_defaults() {
        let raw = "[motion:invalid][exp:invalid] はぁ…何か用？";
        let parsed = parse_llm_response(raw);
        assert_eq!(parsed.motion_id, DEFAULT_MOTION);
        assert_eq!(parsed.expression_id, DEFAULT_EXPRESSION);
        assert_eq!(parsed.clean_text.trim(), "はぁ…何か用？");
    }

    #[test]
    fn test_parse_no_tags() {
        let raw = "plain text without tags";
        let parsed = parse_llm_response(raw);
        assert_eq!(parsed.motion_id, DEFAULT_MOTION);
        assert_eq!(parsed.expression_id, DEFAULT_EXPRESSION);
        assert_eq!(parsed.clean_text, "plain text without tags");
        assert_eq!(parsed.raw_text, raw);
    }

    #[test]
    fn test_parse_only_exp() {
        let raw = "[exp:maid] はい、承知しました。";
        let parsed = parse_llm_response(raw);
        assert_eq!(parsed.motion_id, DEFAULT_MOTION); // motion missing → default
        assert_eq!(parsed.expression_id, "maid");
        assert_eq!(parsed.clean_text.trim(), "はい、承知しました。");
    }

    #[test]
    fn test_parse_diverse_motion_exp() {
        // Test all valid motion/expression combos
        let cases = [
            ("[motion:idle2][exp:happy] Hehe...", "idle2", "happy"),
            ("[motion:alert][exp:predator] ...噛んでもいい？", "alert", "predator"),
            ("[motion:shy_fidget][exp:shy] べ、別に…", "shy_fidget", "shy"),
            (
                "[motion:hangry_sway][exp:hangry] 飴をよこしなさい…",
                "hangry_sway",
                "hangry",
            ),
            (
                "[motion:lazy_stretch][exp:lazy] はぁ…眠い…",
                "lazy_stretch",
                "lazy",
            ),
            (
                "[motion:alert][exp:surprised] えっ…！？",
                "alert",
                "surprised",
            ),
        ];

        for (raw, expected_motion, expected_exp) in cases {
            let parsed = parse_llm_response(raw);
            assert_eq!(
                parsed.motion_id, expected_motion,
                "motion mismatch for: {raw}"
            );
            assert_eq!(
                parsed.expression_id, expected_exp,
                "expression mismatch for: {raw}"
            );
            assert!(
                !parsed.clean_text.contains('['),
                "clean_text still contains tags: {raw}"
            );
        }
    }

    #[test]
    fn test_parse_multiple_tags_uses_first() {
        // If multiple tags appear, the first match should win
        let raw = "[motion:idle][exp:lazy][motion:alert] テキスト";
        let parsed = parse_llm_response(raw);
        assert_eq!(parsed.motion_id, "idle"); // first motion tag
        assert_eq!(parsed.expression_id, "lazy"); // first exp tag
        // All tags should be stripped
        assert!(!parsed.clean_text.contains("[motion:"));
        assert!(!parsed.clean_text.contains("[exp:"));
    }

    #[test]
    fn test_parse_empty_string() {
        let parsed = parse_llm_response("");
        assert_eq!(parsed.motion_id, DEFAULT_MOTION);
        assert_eq!(parsed.expression_id, DEFAULT_EXPRESSION);
        assert_eq!(parsed.clean_text, "");
        assert_eq!(parsed.raw_text, "");
    }

    #[test]
    fn test_parse_whitespace_only() {
        let parsed = parse_llm_response("   ");
        assert_eq!(parsed.motion_id, DEFAULT_MOTION);
        assert_eq!(parsed.expression_id, DEFAULT_EXPRESSION);
        assert_eq!(parsed.clean_text, "   ");
    }

    #[test]
    fn test_parse_with_numbers_and_underscores() {
        // idle2 contains a digit — make sure regex handles it
        let raw = "[motion:idle2][exp:maid2] テスト";
        let parsed = parse_llm_response(raw);
        assert_eq!(parsed.motion_id, "idle2"); // valid motion with number
        assert_eq!(parsed.expression_id, DEFAULT_EXPRESSION); // maid2 is invalid
    }
}
