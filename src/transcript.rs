use anyhow::Result;
use serde_json::Value;
use std::io::{BufRead, Seek, SeekFrom};

/// トランスクリプトJSONLファイルからカーソル位置以降の新しいassistantテキストを読み取る
/// 戻り値: (テキストメッセージのVec, 新しいカーソル位置)
pub fn read_new_assistant_texts(path: &str, cursor: u64) -> Result<(Vec<String>, u64)> {
    let file_size = std::fs::metadata(path)?.len();
    // cursor > filesize の場合はリセット（トランスクリプト再作成等への対応）
    let effective_cursor = if cursor > file_size { 0 } else { cursor };

    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    reader.seek(SeekFrom::Start(effective_cursor))?;

    let mut messages = Vec::new();
    let mut last_complete_pos = effective_cursor;

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }

        // \nで終わらない行は書き込み中の不完全行のためスキップ
        // カーソルはこの行の開始位置（last_complete_pos）に留まる
        if !line.ends_with('\n') {
            break;
        }

        last_complete_pos += n as u64;

        if let Ok(entry) = serde_json::from_str::<Value>(&line) {
            if entry["type"] == "assistant" {
                if let Some(content) = entry["message"]["content"].as_array() {
                    let parts: Vec<&str> = content
                        .iter()
                        .filter_map(|b| {
                            if b["type"] == "text" {
                                b["text"].as_str()
                            } else {
                                None
                            }
                        })
                        .collect();
                    let text = parts.join("\n");
                    if !text.is_empty() {
                        messages.push(text);
                    }
                }
            }
        }
    }

    Ok((messages, last_complete_pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl_file(lines: &[String]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        f.flush().unwrap();
        f
    }

    fn assistant_text_entry(text: &str) -> String {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": text}]
            }
        })
        .to_string()
    }

    fn assistant_tool_use_entry() -> String {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "tool_use", "name": "Bash", "id": "t1"}]
            }
        })
        .to_string()
    }

    fn user_entry(text: &str) -> String {
        serde_json::json!({
            "type": "user",
            "message": {"content": [{"type": "text", "text": text}]}
        })
        .to_string()
    }

    #[test]
    fn test_read_new_assistant_texts() {
        let f = write_jsonl_file(&[assistant_text_entry("hello world")]);
        let (texts, _) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert_eq!(texts, vec!["hello world"]);
    }

    #[test]
    fn test_skip_tool_use_entries() {
        let f = write_jsonl_file(&[assistant_tool_use_entry()]);
        let (texts, _) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert!(texts.is_empty());
    }

    #[test]
    fn test_skip_empty_entries() {
        let entry = serde_json::json!({
            "type": "assistant",
            "message": {"content": []}
        })
        .to_string();
        let f = write_jsonl_file(&[entry]);
        let (texts, _) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert!(texts.is_empty());
    }

    #[test]
    fn test_skip_non_assistant_entries() {
        let f = write_jsonl_file(&[
            user_entry("hi"),
            r#"{"type":"progress","data":{}}"#.to_string(),
            r#"{"type":"system","content":""}"#.to_string(),
        ]);
        let (texts, _) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert!(texts.is_empty());
    }

    #[test]
    fn test_cursor_offset() {
        let line1 = assistant_text_entry("first");
        let line2 = assistant_text_entry("second");
        let first_line_bytes = (line1.len() + 1) as u64; // +1 for \n
        let f = write_jsonl_file(&[line1, line2]);

        let (texts, _) =
            read_new_assistant_texts(f.path().to_str().unwrap(), first_line_bytes).unwrap();
        assert_eq!(texts, vec!["second"]);
    }

    #[test]
    fn test_incomplete_line_skipped() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let complete = assistant_text_entry("complete");
        let incomplete = assistant_text_entry("incomplete");
        write!(f, "{}\n", complete).unwrap();
        write!(f, "{}", incomplete).unwrap(); // \nなし
        f.flush().unwrap();

        let (texts, cursor) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert_eq!(texts, vec!["complete"]);
        // カーソルは完全な行の終わりまで
        let expected_cursor = (complete.len() + 1) as u64; // +1 for \n
        assert_eq!(cursor, expected_cursor);
    }

    #[test]
    fn test_cursor_beyond_filesize_resets() {
        let f = write_jsonl_file(&[assistant_text_entry("text")]);
        // cursor > filesize → 0にリセットして全読み取り
        let (texts, _) = read_new_assistant_texts(f.path().to_str().unwrap(), 99999).unwrap();
        assert_eq!(texts, vec!["text"]);
    }

    #[test]
    fn test_multiple_text_blocks_joined() {
        let entry = serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "text", "text": "part1"},
                    {"type": "tool_use", "name": "Bash"},
                    {"type": "text", "text": "part2"}
                ]
            }
        })
        .to_string();
        let f = write_jsonl_file(&[entry]);
        let (texts, _) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert_eq!(texts, vec!["part1\npart2"]);
    }

    #[test]
    fn test_returns_new_cursor_position() {
        let line1 = assistant_text_entry("msg1");
        let line2 = assistant_text_entry("msg2");
        let expected_bytes = (line1.len() + 1 + line2.len() + 1) as u64;
        let f = write_jsonl_file(&[line1, line2]);

        let (texts, cursor) = read_new_assistant_texts(f.path().to_str().unwrap(), 0).unwrap();
        assert_eq!(texts.len(), 2);
        assert_eq!(cursor, expected_bytes);
    }
}
