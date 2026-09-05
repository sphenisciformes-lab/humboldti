use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};

use crate::notes;

/// `search_notes`/`read_note`/`append_note` を公開する MCP サーバ。
/// 既存の `notes` モジュールをそのまま呼ぶ薄いラッパー
/// (意味検索やベクトル化はまだ導入しない。DESIGN.md の「MCPサーバは
/// 利便性であって本質ではない」の通り)。
#[derive(Clone)]
pub struct PenMcp {
    notes_dir: PathBuf,
    merge_window_minutes: u32,
}

impl PenMcp {
    pub fn new(notes_dir: PathBuf, merge_window_minutes: u32) -> Self {
        Self {
            notes_dir,
            merge_window_minutes,
        }
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchNotesParams {
    /// Case-insensitive regular expression to search for.
    query: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadNoteParams {
    /// Date of the note to read, formatted as YYYY-MM-DD.
    date: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AppendNoteParams {
    /// Text to append to today's note.
    text: String,
}

#[tool_router]
impl PenMcp {
    #[tool(description = "Search notes. The query is a case-insensitive regular expression.")]
    fn search_notes(
        &self,
        Parameters(SearchNotesParams { query }): Parameters<SearchNotesParams>,
    ) -> Result<CallToolResult, McpError> {
        match notes::search(&self.notes_dir, &query) {
            Ok(hits) if hits.is_empty() => Ok(CallToolResult::success(vec![ContentBlock::text(
                "no matches",
            )])),
            Ok(hits) => {
                let text = hits
                    .iter()
                    .map(|h| format!("{}:{}: {}", h.path.display(), h.line_number, h.line))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Err(err) => Ok(CallToolResult::error(vec![ContentBlock::text(
                err.to_string(),
            )])),
        }
    }

    #[tool(description = "Read a specific day's note. Date format: YYYY-MM-DD.")]
    fn read_note(
        &self,
        Parameters(ReadNoteParams { date }): Parameters<ReadNoteParams>,
    ) -> Result<CallToolResult, McpError> {
        let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map_err(|_| McpError::invalid_params("date must be formatted as YYYY-MM-DD", None))?;
        match notes::read_note(&self.notes_dir, date) {
            Ok(Some(content)) => Ok(CallToolResult::success(vec![ContentBlock::text(content)])),
            Ok(None) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "no note found for {date}"
            ))])),
            Err(err) => Ok(CallToolResult::error(vec![ContentBlock::text(
                err.to_string(),
            )])),
        }
    }

    #[tool(description = "Append text to today's note.")]
    fn append_note(
        &self,
        Parameters(AppendNoteParams { text }): Parameters<AppendNoteParams>,
    ) -> Result<CallToolResult, McpError> {
        match notes::append(
            &self.notes_dir,
            &text,
            self.merge_window_minutes,
            Local::now(),
        ) {
            Ok(outcome) => Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "{} ({})",
                outcome.path.display(),
                if outcome.merged {
                    "merged"
                } else {
                    "new heading"
                }
            ))])),
            Err(err) => Ok(CallToolResult::error(vec![ContentBlock::text(
                err.to_string(),
            )])),
        }
    }
}

#[tool_handler(
    name = "pen",
    instructions = "A minimal daily markdown note-taking tool. Notes are one plain \
        markdown file per day."
)]
impl ServerHandler for PenMcp {}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_note(notes_dir: &std::path::Path, date: NaiveDate, contents: &str) {
        let path = notes::note_path(notes_dir, date);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|block| block.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn search_notes_finds_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        write_note(tmp.path(), date, "## 10:00\nMeeting notes\n");
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let result = server
            .search_notes(Parameters(SearchNotesParams {
                query: "meeting".to_string(),
            }))
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        assert!(text_of(&result).contains("Meeting notes"));
    }

    #[test]
    fn search_notes_reports_no_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let result = server
            .search_notes(Parameters(SearchNotesParams {
                query: "nothing".to_string(),
            }))
            .unwrap();

        assert_eq!(text_of(&result), "no matches");
    }

    #[test]
    fn search_notes_reports_invalid_pattern_as_tool_error() {
        let tmp = tempfile::tempdir().unwrap();
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let result = server
            .search_notes(Parameters(SearchNotesParams {
                query: "[".to_string(),
            }))
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn read_note_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        write_note(tmp.path(), date, "## 10:00\n内容\n");
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let result = server
            .read_note(Parameters(ReadNoteParams {
                date: "2026-08-30".to_string(),
            }))
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        assert!(text_of(&result).contains("内容"));
    }

    #[test]
    fn read_note_reports_missing_note_as_tool_error() {
        let tmp = tempfile::tempdir().unwrap();
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let result = server
            .read_note(Parameters(ReadNoteParams {
                date: "2026-08-30".to_string(),
            }))
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn read_note_rejects_bad_date_as_protocol_error() {
        let tmp = tempfile::tempdir().unwrap();
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let err = server
            .read_note(Parameters(ReadNoteParams {
                date: "not-a-date".to_string(),
            }))
            .unwrap_err();

        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn append_note_writes_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let server = PenMcp::new(tmp.path().to_path_buf(), 30);

        let result = server
            .append_note(Parameters(AppendNoteParams {
                text: "経由での追記".to_string(),
            }))
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let today = Local::now().date_naive();
        let contents = std::fs::read_to_string(notes::note_path(tmp.path(), today)).unwrap();
        assert!(contents.contains("経由での追記"));
    }
}
