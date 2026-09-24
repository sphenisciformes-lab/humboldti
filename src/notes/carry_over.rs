use std::path::Path;

use chrono::NaiveDate;

use super::note_path;

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

/// 今日のファイルの先頭に置く繰り越しブロック(末尾の空行込み)。無ければ空。
///
/// 繰り越すのは今日のファイルを今まさに新規作成する瞬間(`contents` が空)
/// だけ。既に何か書かれていれば、その時点で繰り越し済みか、繰り越す物が
/// 無かったかのどちらか。`append` と `open_in_editor` の両方から使う。
/// 見出しはこのブロックより後に置くこと——繰り越しは今日書いたものでは
/// ないので、時刻見出しの対象を「実際に今日書いた内容」だけにする。
pub(super) fn carried_block(notes_dir: &Path, date: NaiveDate, contents: &str) -> String {
    if !contents.is_empty() {
        return String::new();
    }
    let carried = carry_over_items(notes_dir, date);
    if carried.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", carried.join("\n"))
    }
}
