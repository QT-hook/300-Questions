use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub question: String,
    pub options: [String; 4],
    pub answer: char,
}

impl Question {
    pub fn answer_index(&self) -> usize {
        self.answer as usize - 'A' as usize
    }
}

pub fn parse(input: &str) -> Result<Vec<Question>, String> {
    let bank: Vec<Question> =
        serde_json::from_str(input).map_err(|error| format!("题库 JSON 格式错误：{error}"))?;
    if bank.is_empty() {
        return Err("题库不能为空".into());
    }
    for (index, item) in bank.iter().enumerate() {
        if item.question.trim().is_empty()
            || item.options.iter().any(|option| option.trim().is_empty())
            || !matches!(item.answer, 'A'..='D')
        {
            return Err(format!("第 {} 题题干、选项或答案不合法", index + 1));
        }
    }
    Ok(bank)
}

pub fn embedded() -> Result<Vec<Question>, String> {
    parse(include_str!("../assets/questions.json"))
}

/// Returns the lowercase SHA-256 of a validated bank's compact typed JSON.
///
/// Fields are serialized in `Question` declaration order. JSON formatting, key
/// order and equivalent escapes do not matter; record/option order and decoded
/// strings (including internal whitespace and line endings) remain significant.
///
/// # Errors
/// Returns an error for invalid JSON, invalid bank content or serialization failure.
pub fn fingerprint(input: &str) -> Result<String, String> {
    let bank = parse(input)?;
    let canonical =
        serde_json::to_vec(&bank).map_err(|error| format!("题库规范化失败：{error}"))?;
    Ok(format!("{:x}", Sha256::digest(&canonical)))
}

/// Returns the canonical fingerprint of the embedded bank, or its validation error.
pub fn embedded_fingerprint() -> Result<String, String> {
    fingerprint(include_str!("../assets/questions.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shared by all platforms: update only for an intentional bank/schema change.
    const EMBEDDED_FINGERPRINT: &str =
        "d2aa575144f0e3c839d27aac5ae3cfc5426dc7678aeadeb485c4b8c739de0758";

    const SAMPLE: &str = r#"[{"question":"题 😀 / \"quoted\" \\ line\nnext","options":["a","b","c","d"],"answer":"A"}]"#;

    #[test]
    fn embedded_fingerprint_matches_cross_platform_constant_and_format_variants() {
        let source = include_str!("../assets/questions.json");
        let lf = source.replace("\r\n", "\n");
        let bank = parse(source).unwrap();
        for input in [
            source.to_owned(),
            lf.clone(),
            lf.replace('\n', "\r\n"),
            serde_json::to_string(&bank).unwrap(),
            serde_json::to_string_pretty(&bank).unwrap(),
        ] {
            assert_eq!(fingerprint(&input).unwrap(), EMBEDDED_FINGERPRINT);
        }
        assert_eq!(embedded_fingerprint().unwrap(), EMBEDDED_FINGERPRINT);
        println!("embedded canonical SHA-256: {EMBEDDED_FINGERPRINT}");
    }

    #[test]
    fn fingerprint_ignores_format_key_order_and_equivalent_escapes() {
        let reordered_escaped = r#"[
          {"answer":"A", "options":["a","b","c","d"],
           "question":"题 😀 \/ \"quoted\" \\ line\u000anext"}
        ]"#;
        let reordered_escaped = reordered_escaped
            .replace('题', &format!("{}u9898", '\\'))
            .replace('😀', &format!("{}ud83d{}ude00", '\\', '\\'));
        let expected = fingerprint(SAMPLE).unwrap();
        let bank = parse(SAMPLE).unwrap();
        for input in [
            reordered_escaped.to_owned(),
            reordered_escaped.replace('\n', "\r\n"),
            serde_json::to_string_pretty(&bank).unwrap(),
            format!(" \t\r\n{SAMPLE}\r\n"),
        ] {
            assert_eq!(fingerprint(&input).unwrap(), expected, "input: {input}");
        }
    }

    #[test]
    fn fingerprint_preserves_string_content_options_and_answer() {
        let original = parse(SAMPLE).unwrap();
        let expected = fingerprint(SAMPLE).unwrap();
        let mut variants = Vec::new();
        for question in [
            "different".to_owned(),
            format!(" {}", original[0].question),
            format!("{} ", original[0].question),
            original[0].question.replace("line\nnext", "line\r\nnext"),
            original[0].question.replace("line\nnext", "line next"),
        ] {
            let mut changed = original.clone();
            changed[0].question = question;
            variants.push(changed);
        }
        for index in 0..4 {
            let mut changed = original.clone();
            changed[0].options[index].push(' ');
            variants.push(changed);
        }
        let mut reordered = original.clone();
        reordered[0].options.swap(0, 1);
        variants.push(reordered);
        for answer in ['B', 'C', 'D'] {
            let mut changed = original.clone();
            changed[0].answer = answer;
            variants.push(changed);
        }
        for changed in variants {
            let input = serde_json::to_string(&changed).unwrap();
            assert_ne!(fingerprint(&input).unwrap(), expected, "input: {input}");
        }
    }

    #[test]
    fn fingerprint_preserves_question_order_and_duplicate_records() {
        let mut bank = parse(SAMPLE).unwrap();
        let mut second = bank[0].clone();
        second.question = "second".into();
        bank.push(second);
        let expected = fingerprint(&serde_json::to_string(&bank).unwrap()).unwrap();
        bank.swap(0, 1);
        assert_ne!(
            fingerprint(&serde_json::to_string(&bank).unwrap()).unwrap(),
            expected
        );
        bank.swap(0, 1);
        bank.push(bank[0].clone());
        assert_ne!(
            fingerprint(&serde_json::to_string(&bank).unwrap()).unwrap(),
            expected
        );
        bank.truncate(1);
        assert_ne!(
            fingerprint(&serde_json::to_string(&bank).unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn embedded_bank_is_complete_and_retains_distinct_duplicate_records() {
        let bank = embedded().unwrap();
        assert_eq!(bank.len(), 300);
        assert_eq!(bank[274].question, bank[282].question);
        assert_ne!(bank[274].options, bank[282].options);
    }

    #[test]
    fn malformed_or_empty_json_is_rejected() {
        for input in ["", "null", "{}", "[]", "["] {
            assert!(parse(input).is_err(), "accepted {input}");
            assert_eq!(fingerprint(input).unwrap_err(), parse(input).unwrap_err());
        }
    }

    #[test]
    fn schema_and_content_are_strict() {
        for input in [
            r#"[{"question":" ","options":["a","b","c","d"],"answer":"A"}]"#,
            r#"[{"question":"q","options":["a","","c","d"],"answer":"A"}]"#,
            r#"[{"question":"q","options":["a","b","c"],"answer":"A"}]"#,
            r#"[{"question":"q","options":["a","b","c","d"],"answer":"E"}]"#,
            r#"[{"question":"q","options":["a","b","c","d"],"answer":"AB"}]"#,
            r#"[{"question":"q","options":["a","b","c","d"],"answer":"A","extra":1}]"#,
            r#"[{"question":"q","options":["a","b","c","d"]}]"#,
        ] {
            assert!(parse(input).is_err(), "accepted {input}");
            assert_eq!(fingerprint(input).unwrap_err(), parse(input).unwrap_err());
        }
    }
}
