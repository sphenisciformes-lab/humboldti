use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use regex::RegexBuilder;

use super::NotesError;

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: PathBuf,
    pub date: NaiveDate,
    pub line_number: usize,
    pub line: String,
}

/// ファイル名が `YYYY-MM-DD.md` にパースできるものだけをノートとみなす。
/// `attachments/` など無関係なファイルは自然に除外される。
fn note_date_from_path(path: &Path) -> Option<NaiveDate> {
    let stem = path.file_stem()?.to_str()?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

/// `notes_dir` 配下のノートを正規表現(大文字小文字を区別しない)で
/// 行単位に検索する。grep/ripgrep と同じ感覚でクエリをそのまま正規表現
/// として扱う。読み込めないファイルや辿れないエントリは検索全体を
/// 失敗させずに読み飛ばす。
pub fn search(notes_dir: &Path, pattern: &str) -> Result<Vec<SearchHit>, NotesError> {
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|source| NotesError::InvalidPattern {
            pattern: pattern.to_string(),
            source,
        })?;

    let mut hits = Vec::new();
    for entry in ignore::WalkBuilder::new(notes_dir).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(date) = note_date_from_path(entry.path()) else {
            continue;
        };
        let Ok(contents) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for (i, line) in contents.lines().enumerate() {
            if regex.is_match(line) {
                hits.push(SearchHit {
                    path: entry.path().to_path_buf(),
                    date,
                    line_number: i + 1,
                    line: line.to_string(),
                });
            }
        }
    }
    hits.sort_by(|a, b| b.date.cmp(&a.date).then(a.line_number.cmp(&b.line_number)));
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::tests::write_note;

    #[test]
    fn search_is_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        write_note(tmp.path(), date, "## 10:00\nMeeting with the team\n");

        let hits = search(tmp.path(), "meeting").unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].date, date);
        assert_eq!(hits[0].line_number, 2);
        assert_eq!(hits[0].line, "Meeting with the team");
    }

    #[test]
    fn search_sorts_newest_date_first() {
        let tmp = tempfile::tempdir().unwrap();
        let older = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let newer = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        write_note(tmp.path(), older, "## 10:00\ntoken\n");
        write_note(tmp.path(), newer, "## 10:00\ntoken\n");

        let hits = search(tmp.path(), "token").unwrap();

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].date, newer);
        assert_eq!(hits[1].date, older);
    }

    #[test]
    fn search_ignores_files_that_are_not_dated_notes() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        write_note(tmp.path(), date, "## 10:00\ntoken\n");
        let attachments = tmp.path().join("attachments");
        std::fs::create_dir_all(&attachments).unwrap();
        std::fs::write(attachments.join("token.txt"), "token\n").unwrap();

        let hits = search(tmp.path(), "token").unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].date, date);
    }

    #[test]
    fn search_rejects_invalid_regex() {
        let tmp = tempfile::tempdir().unwrap();
        let err = search(tmp.path(), "[").unwrap_err();
        assert!(matches!(err, NotesError::InvalidPattern { .. }));
    }
}
