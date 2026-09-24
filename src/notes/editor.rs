use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{Local, NaiveDate};

use super::carry_over::carried_block;
use super::{NotesError, note_path, open_locked, pending_heading};

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

    // 見出しの差し込みは append() と同じくロックの内側で読んでから書く。
    // ロックはエディタの起動前に手放す(開いている間ずっと `pen <text>` を
    // 待たせないため)。差し込んだ内容は、後で「何も書かれなかったか」を
    // 判定するために `(元の長さ, 差し込み後の内容)` として覚えておく。
    let today = Local::now().date_naive();
    let seeded = if date == today {
        let mut file = open_locked(&path, true).map_err(io_err)?;
        // 読めない理由が非 UTF-8 などなら、何も書かずに中止する。空として
        // 扱うと、見出しだけの内容で上書きしたうえ、何も書かれなければ
        // 「元は空だった」としてファイルごと削除してしまう。
        let mut original = String::new();
        file.read_to_string(&mut original).map_err(io_err)?;
        match pending_heading(&original, merge_window_minutes, Local::now()) {
            Some(heading_line) => {
                // 繰り越し項目も、エディタを開いたときに最初から見えている
                // 状態にしておく。
                let carried = carried_block(notes_dir, date, &original);
                let addition = format!("{carried}{heading_line}");
                // read_to_string でカーソルは既に EOF にあるので、そのまま書けば追記になる。
                file.write_all(addition.as_bytes()).map_err(io_err)?;
                Some((original.len(), format!("{original}{addition}")))
            }
            // 開いた時点で今日のファイルが無ければ見出しは必ず要る
            // (pending_heading は空の内容に対して必ず Some を返す)ので、
            // ここに来るのは既存のファイルだけ。空ファイルは残らない。
            None => None,
        }
    } else {
        None
    };

    let raw_editor = Some(editor)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("EDITOR").ok());
    let editor = editor_or_default(raw_editor.as_deref());
    let status =
        editor_command(editor, &path)
            .status()
            .map_err(|source| NotesError::EditorLaunch {
                command: editor.to_string(),
                source,
            })?;
    if !status.success() {
        return Err(NotesError::EditorExit {
            command: editor.to_string(),
            status,
        });
    }

    if let Some((original_len, seeded)) = seeded {
        // エディタが消していれば、戻すものは無い。
        let mut file = match open_locked(&path, false) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(source) => return Err(io_err(source)),
        };
        let mut after = Vec::new();
        file.read_to_end(&mut after).map_err(io_err)?;
        if after == seeded.as_bytes() {
            // 差し込んだ内容は元の内容の末尾に足しただけなので、元の長さに
            // 切り詰めれば元に戻る。元が空なら、ファイル自体を作らなかった
            // ことにする。削除はロックを持ったまま行う——ロック待ちの追記は
            // open_locked がそれを検知して開き直す。
            if original_len == 0 {
                std::fs::remove_file(&path).map_err(io_err)?;
            } else {
                file.set_len(original_len as u64).map_err(io_err)?;
            }
        }
    }

    Ok(path)
}

/// エディタコマンド(設定の `editor` か `$EDITOR` の値)。未設定/空白だけなら
/// `vi`。環境変数を直接読まない純粋関数にして、テストで env を汚さないようにする。
fn editor_or_default(raw: Option<&str>) -> &str {
    match raw.map(str::trim) {
        Some(editor) if !editor.is_empty() => editor,
        _ => "vi",
    }
}

/// git と同じく `sh -c '<editor> "$@"' <editor> <path>` の形で起動する。
/// シェルに解釈させるので、`code --wait` のような引数付きの指定も、引用符で
/// 囲んだ空白入りのパス(`"/Applications/Sublime Text.app/.../subl" -w`)も、
/// 利用者がシェルに書くのと同じ書き方で通る。空白で機械的に分割すると後者が
/// 壊れる。ノートのパスは `$@` で渡すので、パス自体はシェルに解釈されない。
fn editor_command(editor: &str, path: &Path) -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg(editor)
        .arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_or_default_falls_back_to_vi() {
        assert_eq!(editor_or_default(None), "vi");
        assert_eq!(editor_or_default(Some("")), "vi");
        assert_eq!(editor_or_default(Some("   ")), "vi");
        assert_eq!(editor_or_default(Some(" code --wait ")), "code --wait");
    }

    /// 引数を1行ずつ、`$0` 相当のスクリプト名は除いて、ノートに書き足す
    /// 偽エディタを `dir` に作る。
    fn arg_recording_editor(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        std::fs::create_dir_all(dir).unwrap();
        let script = dir.join("fake editor.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nfor last; do :; done\nfor a; do [ \"$a\" = \"$last\" ] || echo \"arg:$a\" >> \"$last\"; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_names_the_editor_when_it_fails() {
        figment::Jail::expect_with(|jail| {
            let notes_dir = jail.directory().join("notes");
            let date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();

            let err =
                open_in_editor(&notes_dir, date, 30, "no-such-editor-for-pen-tests").unwrap_err();

            // sh 自体は起動できるので、見つからないことは終了コードで分かる。
            assert!(matches!(err, NotesError::EditorExit { .. }));
            assert!(err.to_string().contains("no-such-editor-for-pen-tests"));

            Ok(())
        });
    }

    #[test]
    fn editor_command_accepts_a_quoted_path_with_spaces_and_arguments() {
        let tmp = tempfile::tempdir().unwrap();
        let script = arg_recording_editor(&tmp.path().join("dir with space"));
        // シェルのメタ文字を含むノートのパスも、`$@` 経由なので解釈されない。
        let note = tmp.path().join("my notes $(touch pwned).md");
        std::fs::write(&note, "").unwrap();
        let editor = format!("'{}' --wait 'two words'", script.display());

        let status = editor_command(&editor, &note).status().unwrap();

        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "arg:--wait\narg:two words\n"
        );
        assert!(!tmp.path().join("pwned").exists());
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
    fn open_in_editor_restores_an_existing_note_if_nothing_was_written() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("EDITOR", "true"); // 何も書かずに正常終了するエディタ
            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();
            let path = note_path(&notes_dir, today);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            // マージ期間 0 分なので、00:00 ちょうどでない限り見出しが差し込まれる。
            let original = "## 00:00\n朝のメモ\n";
            std::fs::write(&path, original).map_err(|e| e.to_string())?;

            open_in_editor(&notes_dir, today, 0, "").map_err(|e| e.to_string())?;

            assert_eq!(
                std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
                original
            );

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn open_in_editor_leaves_an_unreadable_todays_note_untouched() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("EDITOR", "true"); // 何も書かずに正常終了するエディタ
            let notes_dir = jail.directory().join("notes");
            let today = Local::now().date_naive();
            let path = note_path(&notes_dir, today);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            // Shift_JIS の「あ」を含む、UTF-8 として読めない内容。
            let original: &[u8] = b"important \x82\xa0\n";
            std::fs::write(&path, original).map_err(|e| e.to_string())?;

            assert!(open_in_editor(&notes_dir, today, 30, "").is_err());
            assert_eq!(std::fs::read(&path).map_err(|e| e.to_string())?, original);

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
}
