use crate::types::Chapter;
use anyhow::{anyhow, Result};
use std::time::{SystemTime, UNIX_EPOCH};

const PDD_AMOUNT_MIN: i64 = 100_000;
const PDD_AMOUNT_MAX: i64 = 10_000_000;

pub fn parse_ai_json(json_str: &str) -> Result<Vec<Chapter>> {
    let cleaned = clean_json_block(json_str);
    match serde_json::from_str::<Vec<Chapter>>(&cleaned) {
        Ok(data) => Ok(data),
        Err(first_err) => {
            let repaired = repair_truncated_json(&cleaned);
            serde_json::from_str::<Vec<Chapter>>(&repaired)
                .map_err(|e| anyhow!("AI JSON parse error: {first_err}; after repair: {e}"))
        }
    }
}

fn clean_json_block(raw: &str) -> String {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix("```json").unwrap_or(trimmed);
    let trimmed = trimmed.strip_prefix("```").unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix("```").unwrap_or(trimmed);
    trimmed.trim().to_string()
}

fn repair_truncated_json(raw: &str) -> String {
    let mut s = raw.trim_end().to_string();
    let mut in_string = false;
    let mut i = 0usize;
    let bytes = s.as_bytes().to_vec();
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch == '\\' && in_string {
            i += 2;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
        }
        i += 1;
    }
    if in_string {
        s.push('"');
    }

    let mut depth = 0i32;
    let mut last_comma_at_depth1: Option<usize> = None;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 1 => last_comma_at_depth1 = Some(idx),
            _ => {}
        }
    }
    if let Some(idx) = last_comma_at_depth1 {
        s.truncate(idx);
    }

    let mut opens: Vec<char> = Vec::new();
    let mut in_str = false;
    for ch in s.chars() {
        if ch == '"' && !in_str {
            in_str = true;
        } else if ch == '"' && in_str {
            in_str = false;
        } else if !in_str {
            match ch {
                '[' => opens.push(']'),
                '{' => opens.push('}'),
                ']' | '}' => {
                    if let Some(last) = opens.last() {
                        if *last == ch {
                            opens.pop();
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for ch in opens.iter().rev() {
        s.push(*ch);
    }
    s
}

pub fn postprocess(
    mode: &str,
    mut data: Vec<Chapter>,
    original: Option<&Vec<Chapter>>,
) -> Vec<Chapter> {
    // Remove any ##updated_at chapters
    data.retain(|ch| ch.name != "##updated_at");

    // If original provided, merge reason/name from original
    if let Some(orig) = original {
        merge_with_original(&mut data, orig);
    }

    if mode != "pdd" {
        return data;
    }

    // For pdd mode, clamp amounts to [PDD_AMOUNT_MIN, PDD_AMOUNT_MAX]
    for chapter in data.iter_mut() {
        for item in chapter.item.iter_mut() {
            if let Some(raw_amount) = &item.amount {
                // Remove spaces and dots/underscores
                let cleaned = raw_amount.replace([' ', '_'], "").replace('.', "");
                let val = cleaned.parse::<i64>().unwrap_or(PDD_AMOUNT_MIN);
                let clamped = val.clamp(PDD_AMOUNT_MIN, PDD_AMOUNT_MAX);
                item.amount = Some(clamped.to_string());
            }
        }
    }

    data
}

pub fn add_updated_at(mut data: Vec<Chapter>) -> Vec<Chapter> {
    data.retain(|ch| ch.name != "##updated_at");
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    data.push(Chapter {
        name: "##updated_at".to_string(),
        item: Vec::new(),
        updated_at: Some(ts),
    });
    data
}

pub fn sanitize_strings(data: &mut [Chapter]) {
    fn clean(s: &str) -> String {
        s.chars()
            .filter(|c| !matches!(c, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' | '\u{00ad}'))
            .collect::<String>()
    }

    for ch in data.iter_mut() {
        ch.name = clean(&ch.name);
        for item in ch.item.iter_mut() {
            item.reason = clean(&item.reason);
            if let Some(name) = item.name.take() {
                item.name = Some(clean(&name));
            }
            if let Some(lvl) = item.lvl.take() {
                item.lvl = Some(clean(&lvl));
            }
            if let Some(amount) = item.amount.take() {
                item.amount = Some(clean(&amount));
            }
        }
    }
}

pub fn merge_with_original(ai_data: &mut Vec<Chapter>, original: &Vec<Chapter>) {
    use std::collections::HashMap;

    let mut orig_reason_map: HashMap<String, String> = HashMap::new();
    let mut orig_chapter_map: HashMap<String, String> = HashMap::new();

    for ch in original.iter() {
        if ch.name == "##updated_at" {
            continue;
        }
        orig_chapter_map.insert(ch.name.to_lowercase(), ch.name.clone());
        for item in ch.item.iter() {
            let r = &item.reason;
            if !r.is_empty() {
                orig_reason_map.insert(r.to_lowercase(), r.clone());
            }
        }
    }

    let mut fixed_reason = 0usize;
    let mut fixed_name = 0usize;

    for ch in ai_data.iter_mut() {
        if ch.name == "##updated_at" {
            continue;
        }
        let ai_ch_name = ch.name.clone();
        if let Some(canonical) = orig_chapter_map.get(&ai_ch_name.to_lowercase()) {
            if &ch.name != canonical {
                fixed_name += 1;
                ch.name = canonical.clone();
            }
        }

        for item in ch.item.iter_mut() {
            let ai_reason = item.reason.clone();
            if let Some(orig_reason) = orig_reason_map.get(&ai_reason.to_lowercase()) {
                if orig_reason != &ai_reason {
                    fixed_reason += 1;
                    item.reason = orig_reason.clone();
                }
            }
        }
    }

    if fixed_name > 0 {
        eprintln!("  Postprocess: restored name for {} chapters", fixed_name);
    }
    if fixed_reason > 0 {
        eprintln!("  Postprocess: restored reason for {} items", fixed_reason);
    }
}

pub fn serialize_output(data: &[Chapter]) -> Result<String> {
    serde_json::to_string_pretty(data)
        .map_err(|e| anyhow!("Serialize error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_truncated_json_closes_array() {
        let raw = r#"[
          {"name":"Глава 1","item":[
            {"reason":"1.1 УК","lvl":"2","text":"Test"},
            {"reason":"1.2 УК","lvl":"3","text":"Test2"}
        "#;

        let repaired = repair_truncated_json(raw);
        let parsed = serde_json::from_str::<Vec<Chapter>>(&repaired);
        assert!(parsed.is_ok());
    }

    #[test]
    fn postprocess_clamps_pdd_amounts() {
        let data = vec![Chapter {
            name: "Глава 1".to_string(),
            item: vec![crate::types::Item {
                reason: "1.1 АК".to_string(),
                name: None,
                lvl: None,
                amount: Some("50.000".to_string()),
            }],
        }];

        let result = postprocess("pdd", data, None);
        assert_eq!(result[0].item[0].amount.as_deref(), Some("100000"));
    }
}
