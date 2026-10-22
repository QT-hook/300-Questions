use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }
}
