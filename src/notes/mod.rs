mod carry_over;
mod context;
mod editor;
mod search;

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime};

pub use context::{ContextOutput, context};
pub use editor::open_in_editor;
pub use search::{SearchHit, search};

use carry_over::carried_block;

#[derive(thiserror::Error, Debug)]
pub enum NotesError {
    #[error("text to append must not be empty")]
    EmptyText,
    #[error("failed to access note file at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to launch editor `{command}`: {source}")]
    EditorLaunch {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("editor exited with a non-zero status: {status}")]
    EditorExit { status: std::process::ExitStatus },
    #[error("invalid search pattern `{pattern}`: {source}")]
    InvalidPattern {
        pattern: String,
        #[source]
        source: regex::Error,
    },
}

#[derive(Debug)]
pub struct AppendOutcome {
    pub path: PathBuf,
    pub heading: String,
    pub merged: bool,
}

pub fn note_path(notes_dir: &Path, date: NaiveDate) -> PathBuf {
    notes_dir
        .join(date.format("%Y").to_string())
        .join(date.format("%m").to_string())
        .join(format!("{}.md", date.format("%Y-%m-%d")))
}

/// `date` のノートを読む。ファイルが無ければ `Ok(None)`
/// (`search`/`append` と違い、`read_note` は利用者が明示的に特定の日を
/// 指定するので、無いことをはっきり区別して返す)。
pub fn read_note(notes_dir: &Path, date: NaiveDate) -> Result<Option<String>, NotesError> {
    let path = note_path(notes_dir, date);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(NotesError::Io { path, source }),
    }
}

/// ファイル末尾から遡って最後の `## HH:MM` 見出しを探す。
fn last_heading(contents: &str) -> Option<(String, NaiveTime)> {
    contents.lines().rev().find_map(|line| {
        let rest = line.strip_prefix("## ")?;
        let time = NaiveTime::parse_from_str(rest, "%H:%M").ok()?;
        Some((rest.to_string(), time))
    })
}

/// 今この瞬間の追記をマージ期間内として既存の見出しの続きに書けるなら、
/// その見出し(`HH:MM`)を返す。
///
/// `append` と、エディタで開く前の見出し差し込み(`open_in_editor`)の
/// 両方から使う判定ロジックなので、ここに1箇所だけ書く。
fn merge_target(contents: &str, merge_window_minutes: u32, now: DateTime<Local>) -> Option<String> {
    let (heading, last_time) = last_heading(contents)?;
    let diff = now.time() - last_time;
    (diff >= Duration::zero() && diff <= Duration::minutes(i64::from(merge_window_minutes)))
        .then_some(heading)
}

/// 今この瞬間に何かを追記するなら、新しい時刻見出しが要るかどうかを判定する。
/// マージ期間内(既存の見出しの続きとして書ける)なら `None`。
/// 要るなら、区切りの改行込みの見出し行(例: `"\n## 21:07\n"`)を返す。
pub fn pending_heading(
    contents: &str,
    merge_window_minutes: u32,
    now: DateTime<Local>,
) -> Option<String> {
    match merge_target(contents, merge_window_minutes, now) {
        Some(_) => None,
        None => Some(new_heading_line(contents, now)),
    }
}

/// `contents` の末尾に足す、区切りの改行込みの新しい時刻見出し行。
fn new_heading_line(contents: &str, now: DateTime<Local>) -> String {
    let sep = if contents.is_empty() { "" } else { "\n" };
    format!("{sep}## {}\n", now.format("%H:%M"))
}

/// `-t`/`--todo` 用に、テキストを未完了チェックリスト行(`- [ ] <text>`)へ
/// 整形する。既に `- [` で始まっている(手で `- [ ]`/`- [x]` を書いた)なら
/// 二重に付けない。空文字はそのまま返す — 空にするかどうかの判断は
/// `append` 側の `EmptyText` チェックに任せる。
pub fn as_todo(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with("- [") {
        trimmed.to_string()
    } else {
        format!("- [ ] {trimmed}")
    }
}

/// ノートを読み書き両方で開き、排他ロックを取ってから返す。ロック前に読むと、
/// 他プロセスが書き込み中の内容を読んでしまう可能性がある。
///
/// ロック待ちの間に別プロセスがファイルを削除することがある(`open_in_editor`
/// は何も書かれなかった今日のノートを消す)。そのまま使うと、パスから切り離された
/// 古いファイルに書き込んで内容が消えるので、ロック取得後にまだリンクされて
/// いるかを確かめ、消されていたら開き直す。
fn open_locked(path: &Path, create: bool) -> std::io::Result<File> {
    loop {
        let file = OpenOptions::new()
            .create(create)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        file.lock()?;
        if !is_unlinked(&file)? {
            return Ok(file);
        }
    }
}

#[cfg(unix)]
fn is_unlinked(file: &File) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(file.metadata()?.nlink() == 0)
}

// Windows は開いているファイルを既定で削除できないので、この競合は起きない。
#[cfg(not(unix))]
fn is_unlinked(_file: &File) -> std::io::Result<bool> {
    Ok(false)
}

/// `notes_dir` 配下の今日のファイルにテキストを追記する。`merge_window_minutes`
/// 以内の連続した追記は、新しい見出しを作らず既存の見出しの下にまとめる。
///
/// `now` を呼び出し側から渡すことで、マージ判定を実際のシステム時刻から
/// 切り離してテストできるようにしている。
pub fn append(
    notes_dir: &Path,
    text: &str,
    merge_window_minutes: u32,
    now: DateTime<Local>,
) -> Result<AppendOutcome, NotesError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(NotesError::EmptyText);
    }

    let path = note_path(notes_dir, now.date_naive());
    let io_err = |source: std::io::Error| NotesError::Io {
        path: path.clone(),
        source,
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err)?;
    }

    let mut file = open_locked(&path, true).map_err(io_err)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(io_err)?;

    let (to_append, heading, merged) = match merge_target(&contents, merge_window_minutes, now) {
        Some(heading) => (format!("\n{text}\n"), heading, true),
        None => {
            let heading_line = new_heading_line(&contents, now);
            let heading = now.format("%H:%M").to_string();
            let carried = carried_block(notes_dir, now.date_naive(), &contents);
            (format!("{carried}{heading_line}{text}\n"), heading, false)
        }
    };

    // read_to_string でカーソルは既に EOF にあるので、そのまま書けば追記になる。
    file.write_all(to_append.as_bytes()).map_err(io_err)?;

    Ok(AppendOutcome {
        path,
        heading,
        merged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 8, 30, h, m, 0).unwrap()
    }

    #[test]
    fn note_path_uses_year_month_day_layout() {
        let dir = Path::new("/notes");
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        assert_eq!(
            note_path(dir, date),
            PathBuf::from("/notes/2026/08/2026-08-30.md")
        );
    }

    #[test]
    fn read_note_returns_content_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let path = note_path(tmp.path(), date);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "## 10:00\n内容\n").unwrap();

        assert_eq!(
            read_note(tmp.path(), date).unwrap(),
            Some("## 10:00\n内容\n".to_string())
        );
    }

    #[test]
    fn read_note_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();

        assert_eq!(read_note(tmp.path(), date).unwrap(), None);
    }

    #[test]
    fn first_append_creates_file_with_heading() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = append(tmp.path(), "最初の思いつき", 30, at(21, 7)).unwrap();
        assert!(!outcome.merged);
        assert_eq!(outcome.heading, "21:07");

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(contents, "## 21:07\n最初の思いつき\n");
    }

    #[test]
    fn append_carries_over_unchecked_items_from_previous_day() {
        let tmp = tempfile::tempdir().unwrap();
        let yesterday = NaiveDate::from_ymd_opt(2026, 8, 29).unwrap();
        std::fs::create_dir_all(note_path(tmp.path(), yesterday).parent().unwrap()).unwrap();
        std::fs::write(
            note_path(tmp.path(), yesterday),
            "## 10:00\n- [ ] 未完了タスク\n- [x] 完了済みタスク\n- [ ] もう1つの未完了\n",
        )
        .unwrap();

        let outcome = append(tmp.path(), "今日の最初のメモ", 30, at(21, 7)).unwrap();

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(
            contents,
            "<!-- carried over from 2026-08-29 -->\n- [ ] 未完了タスク\n- [ ] もう1つの未完了\n\n## 21:07\n今日の最初のメモ\n"
        );
    }

    #[test]
    fn append_preserves_original_carry_over_date_across_multiple_days() {
        // 8/28 に書かれた項目が未完了のまま 8/29 に繰り越され、8/29 でも
        // 未完了のまま今日(8/30、`at()` の固定日)に繰り越される。8/29 で
        // 新しく書かれた項目と混ざっても、初出日はそれぞれ別々のまま
        // 保たれるべき。
        let tmp = tempfile::tempdir().unwrap();
        let two_days_ago = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let yesterday = NaiveDate::from_ymd_opt(2026, 8, 29).unwrap();
        std::fs::create_dir_all(note_path(tmp.path(), yesterday).parent().unwrap()).unwrap();
        std::fs::write(
            note_path(tmp.path(), yesterday),
            format!(
                "<!-- carried over from {} -->\n- [ ] 古い未完了\n\n## 21:48\n- [ ] 昨日の新しい未完了\n",
                two_days_ago.format("%Y-%m-%d")
            ),
        )
        .unwrap();

        let outcome = append(tmp.path(), "今日の最初のメモ", 30, at(21, 7)).unwrap();

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(
            contents,
            "<!-- carried over from 2026-08-28 -->\n- [ ] 古い未完了\n<!-- carried over from 2026-08-29 -->\n- [ ] 昨日の新しい未完了\n\n## 21:07\n今日の最初のメモ\n"
        );
    }

    #[test]
    fn append_looks_back_past_days_with_no_note() {
        let tmp = tempfile::tempdir().unwrap();
        // 8/26〜8/29 はファイルが無く、8/25 まで遡って見つかる想定。
        let older = NaiveDate::from_ymd_opt(2026, 8, 25).unwrap();
        std::fs::create_dir_all(note_path(tmp.path(), older).parent().unwrap()).unwrap();
        std::fs::write(note_path(tmp.path(), older), "## 09:00\n- [ ] 積み残し\n").unwrap();

        let outcome = append(tmp.path(), "今日の最初のメモ", 30, at(21, 7)).unwrap();

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(
            contents,
            "<!-- carried over from 2026-08-25 -->\n- [ ] 積み残し\n\n## 21:07\n今日の最初のメモ\n"
        );
    }

    #[test]
    fn append_does_not_carry_over_on_second_append_of_the_day() {
        let tmp = tempfile::tempdir().unwrap();
        let yesterday = NaiveDate::from_ymd_opt(2026, 8, 29).unwrap();
        std::fs::create_dir_all(note_path(tmp.path(), yesterday).parent().unwrap()).unwrap();
        std::fs::write(note_path(tmp.path(), yesterday), "## 10:00\n- [ ] 未完了\n").unwrap();

        append(tmp.path(), "最初", 30, at(21, 7)).unwrap();
        let outcome = append(tmp.path(), "2つ目", 30, at(22, 0)).unwrap();

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(
            contents,
            "<!-- carried over from 2026-08-29 -->\n- [ ] 未完了\n\n## 21:07\n最初\n\n## 22:00\n2つ目\n"
        );
    }

    #[test]
    fn append_within_merge_window_reuses_heading() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), "最初の思いつき", 30, at(21, 7)).unwrap();
        let outcome = append(tmp.path(), "マージ期間内の2つ目", 30, at(21, 20)).unwrap();

        assert!(outcome.merged);
        assert_eq!(outcome.heading, "21:07");

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(
            contents,
            "## 21:07\n最初の思いつき\n\nマージ期間内の2つ目\n"
        );
    }

    #[test]
    fn append_after_merge_window_starts_new_heading() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), "最初の思いつき", 30, at(21, 7)).unwrap();
        let outcome = append(tmp.path(), "期間を過ぎた3つ目", 30, at(22, 0)).unwrap();

        assert!(!outcome.merged);
        assert_eq!(outcome.heading, "22:00");

        let contents = std::fs::read_to_string(&outcome.path).unwrap();
        assert_eq!(
            contents,
            "## 21:07\n最初の思いつき\n\n## 22:00\n期間を過ぎた3つ目\n"
        );
    }

    #[test]
    fn empty_text_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let err = append(tmp.path(), "   ", 30, at(21, 7)).unwrap_err();
        assert!(matches!(err, NotesError::EmptyText));
    }

    #[test]
    fn as_todo_prefixes_plain_text() {
        assert_eq!(as_todo("buy milk"), "- [ ] buy milk");
        assert_eq!(as_todo("  buy milk  "), "- [ ] buy milk");
    }

    #[test]
    fn as_todo_does_not_double_prefix_an_existing_checklist_item() {
        assert_eq!(as_todo("- [ ] buy milk"), "- [ ] buy milk");
        assert_eq!(as_todo("- [x] already done"), "- [x] already done");
    }

    #[test]
    fn as_todo_leaves_empty_text_empty() {
        assert_eq!(as_todo(""), "");
        assert_eq!(as_todo("   "), "");
    }

    #[test]
    fn open_locked_reopens_a_note_deleted_while_waiting_for_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("2026-08-30.md");
        std::fs::write(&path, "## 21:00\n").unwrap();

        // open_in_editor が何も書かれなかった今日のノートを消すときと同じく、
        // ロックを持ったまま削除する。
        let holder = open_locked(&path, false).unwrap();
        let waiter = {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut file = open_locked(&path, true).unwrap();
                file.write_all(b"new\n").unwrap();
            })
        };
        // 待機側がロック待ちに入るまでの猶予。間に合わず削除後に開いた場合も
        // 期待結果は同じなので、このテストが誤って落ちることはない。
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::remove_file(&path).unwrap();
        drop(holder);
        waiter.join().unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
    }

    pub(super) fn write_note(notes_dir: &Path, date: NaiveDate, contents: &str) {
        let path = note_path(notes_dir, date);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}
