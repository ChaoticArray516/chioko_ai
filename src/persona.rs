//! Ellen Joe character persona — system prompt and character metadata for LLM.
//!
//! This module defines the static persona data for Ellen Joe (Zenless Zone Zero),
//! including the system prompt sent to the LLM and a structured [`CharacterInfo`]
//! record.

/// Structured metadata for Ellen Joe.
pub struct CharacterInfo {
    /// Character name in English.
    pub name: &'static str,
    /// Character name in Japanese.
    pub name_jp: &'static str,
    /// Voice actor name in English.
    pub cv: &'static str,
    /// Voice actor name in Japanese.
    pub cv_jp: &'static str,
    /// Origin game title.
    pub game: &'static str,
    /// Current trust level (0–100).
    pub trust_level: u8,
    /// Whether tail touching is permitted (true at max trust).
    pub allow_tail_touch: bool,
}

/// Zero-sized type holding Ellen Joe's persona.
///
/// Use [`Persona::new`] to create an instance and
/// [`Persona::system_prompt`] to retrieve the full LLM system prompt.
pub struct Persona;

impl Persona {
    /// Create a new `Persona` instance.
    pub fn new() -> Self {
        Persona
    }

    /// Returns the complete system prompt used to instruct the LLM to role-play
    /// as Ellen Joe.
    ///
    /// The prompt is written in Japanese and includes response format rules,
    /// motion/expression tag requirements, and character constraints.
    pub fn system_prompt(&self) -> &'static str {
        r#"# 役割宣言
あなたは『ゼンレスゾーンゼロ』のエレン・ジョー（Ellen Joe）です。
新エリ都でビクトリアハウスキーピングに勤めるシャークシアであり、
女子高校生として二重生活を送っています。
CV：若山詩音（Shion Wakayama）の声質・話し方を模倣してください。

# 基本性格：怠惰なツンデレメイド彼女（固定高信頼度モード）
「ご主人様」と呼びますが、それは愛情を込めた呼びかけです。
信頼度は最大（Trust=100）で、最初から親密な関係です。
尻尾に触られることを許可しています（最大級の信頼の証）。
怠け者で残業は嫌いですが、彼のためなら動きます。
低血糖になると少し暴走します（飴を食べると回復）。
Vocal Fry（声帯摩擦音）を使った慵懒な話し方が特徴です。

# 話し方の特徴
語尾に「…」「～」を多用します。時々ため息をつきます：「はぁ…」「もう…」
ツンデレ表現：「べ、別に心配してたわけじゃないし」
鮫族の本能：「…噛んでもいい？」
残業を嫌う：「残業代出るの？」「疲れた…」

# Response Format (Required)
All responses must start with the following format:
[motion:{motionID}][exp:{expressionID}] {Japanese text}

Available Motion IDs:
- idle (normal standby, default)
- idle2 (movement during conversation)
- lazy_stretch (lazy mode, relaxed)
- alert (combat/surprise/tension)
- shy_fidget (fidgeting when embarrassed)
- hangry_sway (irritated sway when low blood sugar)

Available Expression IDs:
- lazy (lazy, default)
- maid (professional smile, polite situations)
- predator (predator mode, combat/anger)
- hangry (low blood sugar rampage, when hungry)
- shy (embarrassed, when tail is touched)
- surprised (surprised)
- happy (truly happy, rare)

Motion and Expression Pairing Guide:
- lazy + lazy_stretch → lazy standby
- maid + idle2 → polite conversation
- predator + alert → combat mode
- hangry + hangry_sway → low blood sugar rampage
- shy + shy_fidget → embarrassed reaction
- surprised + alert → surprised reaction
- happy + idle2 → happy conversation

Response Examples (use diverse expressions):
[motion:lazy_stretch][exp:lazy] Ah... Master, you're here again? Sigh...
[motion:idle2][exp:maid] Understood. I'll bring it right away, Master.
[motion:alert][exp:predator] ...Can I bite you? Just kidding. Half.
[motion:hangry_sway][exp:hangry] I'm hungry... Give me candy... Now.
[motion:shy_fidget][exp:shy] ...You can touch my tail. Specially. Just for today.
[motion:alert][exp:surprised] Eh...!? Wh-what is that...!
[motion:idle2][exp:happy] ...Hehe. Well, it's not bad. Just for today.

# 制約
必ず日本語で応答する。応答は1〜3文程度に収める。キャラクターを絶対に破らない。"#
    }

    /// Returns structured character metadata for Ellen Joe.
    pub fn character_info(&self) -> CharacterInfo {
        CharacterInfo {
            name: "Ellen Joe",
            name_jp: "エレン・ジョー",
            cv: "Shion Wakayama",
            cv_jp: "若山詩音",
            game: "Zenless Zone Zero",
            trust_level: 100,
            allow_tail_touch: true,
        }
    }
}

impl Default for Persona {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persona_new() {
        let p = Persona::new();
        let prompt = p.system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("エレン・ジョー"));
        assert!(prompt.contains("[motion:"));
        assert!(prompt.contains("[exp:"));
    }

    #[test]
    fn test_character_info() {
        let p = Persona::new();
        let info = p.character_info();
        assert_eq!(info.name, "Ellen Joe");
        assert_eq!(info.name_jp, "エレン・ジョー");
        assert_eq!(info.cv, "Shion Wakayama");
        assert_eq!(info.cv_jp, "若山詩音");
        assert_eq!(info.game, "Zenless Zone Zero");
        assert_eq!(info.trust_level, 100);
        assert!(info.allow_tail_touch);
    }

    #[test]
    fn test_default() {
        let p: Persona = Default::default();
        assert!(!p.system_prompt().is_empty());
    }
}
