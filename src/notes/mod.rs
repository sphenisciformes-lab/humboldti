use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime};
use regex::RegexBuilder;

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

/// 日本語・英語混在を想定した安全側(少なめ)の概算。実際のモデルの
/// トークナイザとは一致しない。1トークンあたり平均2文字と仮定する
/// (英語は1トークン≒4文字、日本語のCJKは1トークン≒1〜2文字になりやすく、
/// その中間よりやや安全側に寄せた値)。
fn estimate_tokens(s: &str) -> usize {
    s.chars().count().div_ceil(2)
}

#[derive(Debug)]
pub struct ContextOutput {
    /// 古い日付が先。
    pub days: Vec<(NaiveDate, String)>,
    pub estimated_tokens: usize,
}

/// `today` から `since_days` 日分(当日を含む)のノートを、新しい日から
/// 遡って `max_tokens` の概算予算に収まるだけ集める。ファイルの途中では
/// 切らない——1日単位で入れるか入れないかを決める。ただし直近1日だけで
/// 既に予算を超える場合は、空を返すより実用的なのでその1日だけ返す。
pub fn context(
    notes_dir: &Path,
    since_days: u32,
    max_tokens: usize,
    today: NaiveDate,
) -> ContextOutput {
    let cutoff = today - Duration::days(i64::from(since_days.max(1)) - 1);
    let mut collected = Vec::new();
    let mut total_tokens = 0;
    let mut date = today;
    loop {
        if date < cutoff {
            break;
        }
        if let Ok(contents) = std::fs::read_to_string(note_path(notes_dir, date)) {
            let tokens = estimate_tokens(&contents);
            if !collected.is_empty() && total_tokens + tokens > max_tokens {
                break;
            }
            collected.push((date, contents));
            total_tokens += tokens;
            if total_tokens > max_tokens {
                break;
            }
        }
        match date.pred_opt() {
            Some(prev) => date = prev,
            None => break,
        }
    }
    collected.reverse();
    ContextOutput {
        days: collected,
        estimated_tokens: total_tokens,
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

/// 今この瞬間に何かを追記するなら、新しい時刻見出しが要るかどうかを判定する。
/// マージ期間内(既存の見出しの続きとして書ける)なら `None`。
/// 要るなら、区切りの改行込みの見出し行(例: `"\n## 21:07\n"`)を返す。
///
/// `append` と、エディタで開く前の見出し差し込み(`main.rs` の `run_open`)の
/// 両方から使う判定ロジックなので、ここに1箇所だけ書く。
pub fn pending_heading(
    contents: &str,
    merge_window_minutes: u32,
    now: DateTime<Local>,
) -> Option<String> {
    let merge = last_heading(contents).is_some_and(|(_, last_time)| {
        let diff = now.time() - last_time;
        diff >= Duration::zero() && diff <= Duration::minutes(i64::from(merge_window_minutes))
    });

    if merge {
        None
    } else {
        let sep = if contents.is_empty() { "" } else { "\n" };
        Some(format!("{sep}## {}\n", now.format("%H:%M")))
    }
}

/// これより遡って未完了タスクを探す上限。実際に見つからなければそれ以上
/// 遡らないが、壊れた設定などで際限なく stat し続けないための保険。
const CARRY_OVER_LOOKBACK_DAYS: i64 = 365;

/// `<!-- carried over from YYYY-MM-DD -->` から日付部分だけを取り出す。
fn parse_carried_over_comment(line: &str) -> Option<NaiveDate> {
    let rest = line.trim().strip_prefix("<!-- carried over from ")?;
    let date_str = rest.strip_suffix(" -->")?;
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
}

/// `date` より前で直近にノートが存在する日を遡って探し、その中の未完了
/// `- [ ]` 行を集めて返す。見つからなければ空。各項目の直前に、どの日から
/// 繰り越したかを示す `<!-- carried over from YYYY-MM-DD -->` の1行を添える
/// (`pen context` の `<!-- estimated tokens: ... -->` と同じ、ツールが挿入
/// したメタ情報であって本文ではないことを示す記法)。
///
/// 「前日」ではなく「直近にファイルがある日」まで遡るのは、数日書かずに
/// 空けたときにその間の未完了タスクを取りこぼさないため。チェック済みの
/// `- [x]` はここでは対象にならない——完了しているので繰り越す理由がない。
///
/// 遡った先のファイル自身が、さらに古い日から繰り越された項目を含んで
/// いることがある(何日も未完了のまま持ち越されている項目)。その場合は
/// そのファイルの日付ではなく、元々の初出日をそのまま引き継ぐ。既存の
/// `<!-- carried over from X -->` コメントは時刻見出しの手前までしか
/// 効かない——見出しより後は、そのファイルの日付で新しく書かれた内容だと
/// 分かっているため。同じ初出日が連続する項目は1つのコメントにまとめる。
fn carry_over_items(notes_dir: &Path, date: NaiveDate) -> Vec<String> {
    let mut cursor = date;
    for _ in 0..CARRY_OVER_LOOKBACK_DAYS {
        cursor = match cursor.pred_opt() {
            Some(d) => d,
            None => return Vec::new(),
        };
        if let Ok(contents) = std::fs::read_to_string(note_path(notes_dir, cursor)) {
            let mut origin = cursor;
            let mut groups: Vec<(NaiveDate, Vec<String>)> = Vec::new();
            for line in contents.lines() {
                if let Some(parsed) = parse_carried_over_comment(line) {
                    origin = parsed;
                    continue;
                }
                if line.starts_with("## ") {
                    origin = cursor;
                    continue;
                }
                if line.trim_start().starts_with("- [ ]") {
                    match groups.last_mut() {
                        Some((last_origin, items)) if *last_origin == origin => {
                            items.push(line.to_string());
                        }
                        _ => groups.push((origin, vec![line.to_string()])),
                    }
                }
            }
            if groups.is_empty() {
                return Vec::new();
            }
            let mut carried = Vec::new();
            for (origin, items) in groups {
                carried.push(format!(
                    "<!-- carried over from {} -->",
                    origin.format("%Y-%m-%d")
                ));
                carried.extend(items);
            }
            return carried;
        }
    }
    Vec::new()
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

    // 読み書き両方で開き、ロックを取ってから読む。ロック前に読むと、
    // 他プロセスが書き込み中の内容を読んでしまう可能性がある。
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(io_err)?;
    file.lock().map_err(io_err)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(io_err)?;

    let (to_append, heading, merged) = match pending_heading(&contents, merge_window_minutes, now) {
        Some(heading_line) => {
            let heading = now.format("%H:%M").to_string();
            // 今日のファイルを今まさに新規作成する瞬間だけ、前日以前から
            // 未完了タスクを繰り越す。
            let carried = if contents.is_empty() {
                carry_over_items(notes_dir, now.date_naive())
            } else {
                Vec::new()
            };
            let carried_block = if carried.is_empty() {
                String::new()
            } else {
                format!("{}\n\n", carried.join("\n"))
            };
            // 見出しは繰り越し項目より後に置く。繰り越しは今日書いたもの
            // ではないので、時刻見出しの対象を「実際に今日書いた内容」
            // だけにする——見た目の区切りにもなる。
            (
                format!("{carried_block}{heading_line}{text}\n"),
                heading,
                false,
            )
        }
        None => {
            let (heading, _) =
                last_heading(&contents).expect("no pending heading implies a previous one exists");
            (format!("\n{text}\n"), heading, true)
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

/// `date` のノートを、設定された `editor`(空なら `$EDITOR`、それも無ければ
/// `vi`)で開く。開いた結果のパスを返す。`pen open` とカレンダー画面の
/// Enter の両方から使う共通ロジックなので、ここに1箇所だけ書く。
///
/// 今日のノートを開くときだけ、マージ期間外なら開く前に見出しを差し込む。
/// エディタが何も書き足さずに終了したら元の状態に戻す(空の見出しだけの
/// ファイルを残さないため)。過去/未来の日はその日の実際の記入時刻が
/// 分からないので対象外。
pub fn open_in_editor(
    notes_dir: &Path,
    date: NaiveDate,
    merge_window_minutes: u32,
    editor: &str,
) -> Result<PathBuf, NotesError> {
    let path = note_path(notes_dir, date);
    let io_err = |source: std::io::Error| NotesError::Io {
        path: path.clone(),
        source,
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err)?;
    }

    let today = Local::now().date_naive();
    let original = if date == today {
        Some(std::fs::read_to_string(&path).unwrap_or_default())
    } else {
        None
    };
    let seeded = original.as_ref().and_then(|original| {
        pending_heading(original, merge_window_minutes, Local::now()).map(|heading_line| {
            // 今日のファイルを今まさに新規作成する瞬間だけ、前日以前から
            // 未完了タスクを繰り越しておく。エディタを開いたときに
            // 最初から見えている状態にする。
            let carried = if original.is_empty() {
                carry_over_items(notes_dir, date)
            } else {
                Vec::new()
            };
            let carried_block = if carried.is_empty() {
                String::new()
            } else {
                format!("{}\n\n", carried.join("\n"))
            };
            // append() と同じ理由で、見出しは繰り越し項目より後に置く。
            format!("{original}{carried_block}{heading_line}")
        })
    });
    if let Some(seeded) = &seeded {
        std::fs::write(&path, seeded).map_err(io_err)?;
    }

    let raw_editor = Some(editor)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("EDITOR").ok());
    let editor = parse_editor_command(raw_editor.as_deref());
    let status = std::process::Command::new(&editor[0])
        .args(&editor[1..])
        .arg(&path)
        .status()
        .map_err(|source| NotesError::EditorLaunch {
            command: editor[0].clone(),
            source,
        })?;
    if !status.success() {
        return Err(NotesError::EditorExit { status });
    }

    if let Some(seeded) = seeded {
        let after = std::fs::read_to_string(&path).unwrap_or_default();
        if after == seeded {
            match original.filter(|o| !o.is_empty()) {
                Some(original) => std::fs::write(&path, original).map_err(io_err)?,
                None => std::fs::remove_file(&path).map_err(io_err)?,
            }
        }
    }

    Ok(path)
}

/// エディタコマンド(設定の `editor` か `$EDITOR` の値)を空白区切りで
/// コマンド+引数に分解する。未設定/空なら `vi`。環境変数を直接読まない
/// 純粋関数にして、テストで env を汚さないようにする。
fn parse_editor_command(raw: Option<&str>) -> Vec<String> {
    let parts: Vec<String> = raw
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        vec!["vi".to_string()]
    } else {
        parts
    }
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
    fn parse_editor_command_defaults_to_vi() {
        assert_eq!(parse_editor_command(None), vec!["vi".to_string()]);
        assert_eq!(parse_editor_command(Some("")), vec!["vi".to_string()]);
        assert_eq!(parse_editor_command(Some("   ")), vec!["vi".to_string()]);
    }

    #[test]
    fn parse_editor_command_splits_arguments() {
        assert_eq!(
            parse_editor_command(Some("code --wait")),
            vec!["code".to_string(), "--wait".to_string()]
        );
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_creates_parent_dir_but_not_the_note_itself_for_a_past_date() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("EDITOR", "true");
            let notes_dir = jail.directory().join("notes");
            // 過去日なので見出しの差し込み対象にならない。
            let date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();

            open_in_editor(&notes_dir, date, 30, "").map_err(|e| e.to_string())?;

            let expected_path = note_path(&notes_dir, date);
            assert!(expected_path.parent().unwrap().is_dir());
            assert!(!expected_path.exists());

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_rolls_back_todays_seeded_heading_if_nothing_was_written() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("EDITOR", "true"); // 何も書かずに正常終了するエディタ
            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();

            open_in_editor(&notes_dir, today, 30, "").map_err(|e| e.to_string())?;

            let path = note_path(&notes_dir, today);
            assert!(
                !path.exists(),
                "何も書かなかったのに見出しだけのファイルが残っている"
            );

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_rolls_back_even_when_carry_over_items_exist_but_nothing_was_added() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("EDITOR", "true"); // 何も書かずに正常終了するエディタ
            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();
            let yesterday = today.pred_opt().unwrap();
            std::fs::create_dir_all(note_path(&notes_dir, yesterday).parent().unwrap())
                .map_err(|e| e.to_string())?;
            std::fs::write(note_path(&notes_dir, yesterday), "## 10:00\n- [ ] 未完了\n")
                .map_err(|e| e.to_string())?;

            open_in_editor(&notes_dir, today, 30, "").map_err(|e| e.to_string())?;

            // 繰り越し項目をエディタに表示はしたが、何も新しく書かずに
            // 終了したので、今日のファイルは作られない
            // (前日のファイルには元のまま残っている)。
            let path = note_path(&notes_dir, today);
            assert!(!path.exists());

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_seeds_carried_over_items_when_something_is_written() {
        use std::os::unix::fs::PermissionsExt;

        figment::Jail::expect_with(|jail| {
            jail.create_file("fake_editor.sh", "#!/bin/sh\necho '新しいメモ' >> \"$1\"\n")?;
            let editor_path = jail.directory().join("fake_editor.sh");
            std::fs::set_permissions(&editor_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
            jail.set_env("EDITOR", editor_path.display());

            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();
            let yesterday = today.pred_opt().unwrap();
            std::fs::create_dir_all(note_path(&notes_dir, yesterday).parent().unwrap())
                .map_err(|e| e.to_string())?;
            std::fs::write(note_path(&notes_dir, yesterday), "## 10:00\n- [ ] 未完了\n")
                .map_err(|e| e.to_string())?;

            open_in_editor(&notes_dir, today, 30, "").map_err(|e| e.to_string())?;

            let path = note_path(&notes_dir, today);
            let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            assert!(contents.contains("- [ ] 未完了"));
            assert!(contents.contains("新しいメモ"));
            assert!(contents.contains(&format!(
                "<!-- carried over from {} -->",
                yesterday.format("%Y-%m-%d")
            )));

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_keeps_seeded_heading_when_editor_writes_something() {
        use std::os::unix::fs::PermissionsExt;

        figment::Jail::expect_with(|jail| {
            // 「エディタ」として、渡されたファイルに1行追記して保存したふりを
            // するシェルスクリプトを使う。
            jail.create_file("fake_editor.sh", "#!/bin/sh\necho '思いつき' >> \"$1\"\n")?;
            let editor_path = jail.directory().join("fake_editor.sh");
            std::fs::set_permissions(&editor_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
            jail.set_env("EDITOR", editor_path.display());

            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();

            open_in_editor(&notes_dir, today, 30, "").map_err(|e| e.to_string())?;

            let path = note_path(&notes_dir, today);
            let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            assert!(contents.starts_with("## "));
            assert!(contents.contains("思いつき"));

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_prefers_configured_editor_over_editor_env_var() {
        use std::os::unix::fs::PermissionsExt;

        figment::Jail::expect_with(|jail| {
            // $EDITOR と設定の editor、両方をそれぞれ別の文言を書き込む
            // フェイクエディタにしておき、設定側が使われたことを見分ける。
            jail.create_file(
                "from_env.sh",
                "#!/bin/sh\necho '環境変数のエディタ' >> \"$1\"\n",
            )?;
            jail.create_file(
                "from_config.sh",
                "#!/bin/sh\necho '設定のエディタ' >> \"$1\"\n",
            )?;
            let env_editor = jail.directory().join("from_env.sh");
            let config_editor = jail.directory().join("from_config.sh");
            for path in [&env_editor, &config_editor] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
            jail.set_env("EDITOR", env_editor.display());

            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();

            open_in_editor(&notes_dir, today, 30, &config_editor.display().to_string())
                .map_err(|e| e.to_string())?;

            let contents =
                std::fs::read_to_string(note_path(&notes_dir, today)).map_err(|e| e.to_string())?;
            assert!(contents.contains("設定のエディタ"));
            assert!(!contents.contains("環境変数のエディタ"));

            Ok(())
        });
    }

    fn write_note(notes_dir: &Path, date: NaiveDate, contents: &str) {
        let path = note_path(notes_dir, date);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

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

    #[test]
    fn estimate_tokens_rounds_up_half_a_token() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("ab"), 1);
        assert_eq!(estimate_tokens("abc"), 2);
        assert_eq!(estimate_tokens("日本語"), 2);
    }

    #[test]
    fn context_collects_days_within_range_oldest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "today");
        write_note(tmp.path(), today.pred_opt().unwrap(), "yesterday");
        let out_of_range = today - Duration::days(5);
        write_note(tmp.path(), out_of_range, "too old");

        let out = context(tmp.path(), 3, 10_000, today);

        assert_eq!(out.days.len(), 2);
        assert_eq!(out.days[0].0, today.pred_opt().unwrap());
        assert_eq!(out.days[1].0, today);
    }

    #[test]
    fn context_skips_days_with_no_note() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "today");
        // today - 1 は意図的に書かない。
        write_note(tmp.path(), today - Duration::days(2), "two days ago");

        let out = context(tmp.path(), 3, 10_000, today);

        assert_eq!(out.days.len(), 2);
        assert_eq!(out.days[0].0, today - Duration::days(2));
        assert_eq!(out.days[1].0, today);
    }

    #[test]
    fn context_stops_at_a_day_boundary_when_budget_would_be_exceeded() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        // "today" 単体で概算5トークン、"yesterday" を足すと超える予算にする。
        write_note(tmp.path(), today, "0123456789"); // 10 chars -> 5 tokens
        write_note(tmp.path(), today.pred_opt().unwrap(), "0123456789");

        let out = context(tmp.path(), 7, 6, today);

        assert_eq!(out.days.len(), 1);
        assert_eq!(out.days[0].0, today);
        assert_eq!(out.estimated_tokens, 5);
    }

    #[test]
    fn context_returns_a_single_day_even_if_it_alone_exceeds_the_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "0123456789"); // 10 chars -> 5 tokens

        let out = context(tmp.path(), 7, 1, today);

        assert_eq!(out.days.len(), 1);
        assert_eq!(out.days[0].0, today);
        assert_eq!(out.estimated_tokens, 5);
    }
}
