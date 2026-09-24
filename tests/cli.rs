//! 実際の `pen` バイナリを起動する結合テスト。単体テストでは見えない、
//! clap の解釈・プロセス間のロック・終了コードまで含めた振る舞いを確かめる。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::Local;
use serde_json::Value;

/// 利用者の環境(`~/notes`、実際の設定ファイル、`PEN_*`、`$EDITOR`)に
/// 一切触れない `pen` コマンド。設定ディレクトリも一時ディレクトリに向ける。
fn pen(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pen"));
    cmd.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .arg("--dir")
        .arg(home.join("notes"));
    cmd
}

fn run(cmd: &mut Command) -> Output {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "pen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn todays_note(home: &Path) -> PathBuf {
    let today = Local::now().date_naive();
    home.join("notes")
        .join(today.format("%Y/%m/%Y-%m-%d.md").to_string())
}

#[test]
fn appended_text_can_be_found_by_search() {
    let home = tempfile::tempdir().unwrap();

    let first = json(&run(pen(home.path()).args(["--json", "牛乳を買った"])));
    let second = json(&run(pen(home.path()).args(["--json", "卵も"])));
    assert_eq!(first["merged"], false);
    assert_eq!(second["merged"], true);

    let found = json(&run(pen(home.path()).args(["--json", "search", "牛乳"])));
    let hits = found["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["line"], "牛乳を買った");
}

#[test]
fn double_dash_appends_text_that_starts_with_a_subcommand_name() {
    let home = tempfile::tempdir().unwrap();

    run(pen(home.path()).args(["--", "config", "the", "server"]));

    let contents = std::fs::read_to_string(todays_note(home.path())).unwrap();
    assert!(contents.contains("config the server"));
}

// CLAUDE.md: 2つのターミナルから同時に `pen <text>` を実行してもファイルが
// 壊れないこと。スレッドではなく別プロセスで、実際のロックを通す。
#[test]
fn concurrent_appends_from_separate_processes_are_all_kept() {
    let home = tempfile::tempdir().unwrap();

    let children: Vec<_> = (0..8)
        .map(|i| {
            pen(home.path())
                .arg(format!("同時書き込み-{i}"))
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }

    let contents = std::fs::read_to_string(todays_note(home.path())).unwrap();
    for i in 0..8 {
        assert_eq!(
            contents.matches(&format!("同時書き込み-{i}\n")).count(),
            1,
            "{i} 番目の追記が欠けているか重複している:\n{contents}"
        );
    }
}

#[test]
fn open_leaves_an_unreadable_note_untouched() {
    let home = tempfile::tempdir().unwrap();
    let path = todays_note(home.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Shift_JIS の「あ」を含む、UTF-8 として読めない内容。
    let original: &[u8] = b"important \x82\xa0\n";
    std::fs::write(&path, original).unwrap();

    let output = pen(home.path())
        .env("EDITOR", "true")
        .arg("open")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
