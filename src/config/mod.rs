use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;
use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

use crate::action::{self, Mode};

const APP_DIR_NAME: &str = "pen";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("could not determine the config directory")]
    NoConfigDir(#[from] etcetera::HomeDirError),
    #[error("failed to load config from {path}: {source}")]
    Load {
        path: PathBuf,
        #[source]
        source: Box<figment::Error>,
    },
    #[error("failed to write config file at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("config file already exists at {0}")]
    AlreadyExists(PathBuf),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_expanded_path")]
    pub notes_dir: PathBuf,
    /// 何分以内の連続した追記なら、新しい時刻見出しを作らず
    /// 既存の見出しの下にまとめるか。
    #[serde(default = "default_merge_window_minutes")]
    pub merge_window_minutes: u32,
    /// `pen`/`pen open`/`pen cal` の Enter で使うエディタコマンド(引数込み)。
    /// 空文字列(既定)は「未設定」を意味し、`$EDITOR`、それも無ければ `vi`
    /// にフォールバックする。このフォールバック自体は `notes::open_in_editor`
    /// の実装側にあり、ここでは「設定が無い」ことだけを表す。
    #[serde(default)]
    pub editor: String,
    #[serde(default)]
    pub keys: KeysConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            notes_dir: expand_tilde("~/notes"),
            merge_window_minutes: 30,
            editor: String::new(),
            keys: KeysConfig::default(),
        }
    }
}

// #[serde(default = ...)] は「設定ファイルにキーが無くても動く」ためのもので、
// 既定値そのものの定義ではない。値自体は Default::default() の 30 と一致させる。
fn default_merge_window_minutes() -> u32 {
    Config::default().merge_window_minutes
}

/// `[keys.<mode>]` ごとのアクション名→キー仕様表。実際の既定値は
/// `action::default_bindings` が唯一の情報源で、ここではそれを呼ぶだけにする。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct KeysConfig {
    #[serde(default = "default_calendar_keys")]
    pub calendar: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_search_input_keys")]
    pub search_input: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_search_results_keys")]
    pub search_results: BTreeMap<String, Vec<String>>,
}

impl Default for KeysConfig {
    fn default() -> Self {
        KeysConfig {
            calendar: default_calendar_keys(),
            search_input: default_search_input_keys(),
            search_results: default_search_results_keys(),
        }
    }
}

fn default_calendar_keys() -> BTreeMap<String, Vec<String>> {
    action::default_bindings(Mode::Calendar)
}

fn default_search_input_keys() -> BTreeMap<String, Vec<String>> {
    action::default_bindings(Mode::SearchInput)
}

fn default_search_results_keys() -> BTreeMap<String, Vec<String>> {
    action::default_bindings(Mode::SearchResults)
}

fn expand_tilde(s: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(s).into_owned())
}

fn deserialize_expanded_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(expand_tilde(&raw))
}

/// XDG のベースディレクトリ（`~/.config` 相当）にアプリ名を足したもの。
/// `BaseStrategy` の解決自体は etcetera 側で既にテストされているので、
/// ここでは「ベースディレクトリにアプリ名を足す」部分だけを純粋関数として
/// 切り出し、単体テストできるようにする。
fn app_config_dir(base: &Path) -> PathBuf {
    base.join(APP_DIR_NAME)
}

pub fn config_dir() -> Result<PathBuf, ConfigError> {
    let strategy = etcetera::choose_base_strategy()?;
    Ok(app_config_dir(&strategy.config_dir()))
}

pub fn config_file_path() -> Result<PathBuf, ConfigError> {
    Ok(config_dir()?.join(CONFIG_FILE_NAME))
}

#[derive(Serialize, Default)]
struct CliOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    notes_dir: Option<PathBuf>,
}

/// CLI フラグ > 環境変数 > 設定ファイル > 既定値、の順で `Config` を組み立てる。
pub fn resolve(cli_dir: Option<PathBuf>) -> Result<Config, ConfigError> {
    let path = config_file_path()?;
    let figment = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file(&path))
        .merge(Env::prefixed("PEN_").map(|key| {
            if key == "DIR" {
                "notes_dir".into()
            } else {
                key.into()
            }
        }))
        .merge(Serialized::defaults(CliOverrides { notes_dir: cli_dir }));

    figment.extract().map_err(|source| ConfigError::Load {
        path,
        source: Box::new(source),
    })
}

/// 設定ファイルに `Config` が知らないキーがあれば警告する（起動は止めない）。
/// `keys.<mode>` の中のアクション名も含めて再帰的に比較する
/// (`[keys.calendar]` の下でタイポしたアクション名は、黙って無視されるより
/// 警告される方が親切)。
pub fn warn_unknown_keys() -> Result<(), ConfigError> {
    let path = config_file_path()?;
    if !path.exists() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(&path).map_err(|source| ConfigError::Write {
        path: path.clone(),
        source,
    })?;
    let raw_table: toml::Value = raw
        .parse()
        .map_err(|e: toml::de::Error| ConfigError::Load {
            path: path.clone(),
            source: Box::new(figment::Error::from(e.to_string())),
        })?;
    let known_table =
        toml::Value::try_from(Config::default()).expect("Config always serializes to a TOML table");

    warn_unknown_table_keys(&raw_table, &known_table, "", &path);

    Ok(())
}

/// `raw` にあって `known` に無いキーを、ネストしたテーブルも辿って警告する。
fn warn_unknown_table_keys(raw: &toml::Value, known: &toml::Value, prefix: &str, path: &Path) {
    let (toml::Value::Table(raw), toml::Value::Table(known)) = (raw, known) else {
        return;
    };

    for (key, raw_value) in raw {
        let full_key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match known.get(key) {
            None => eprintln!(
                "warning: unknown config key \"{full_key}\" in {}, ignoring",
                path.display()
            ),
            Some(known_value) => {
                warn_unknown_table_keys(raw_value, known_value, &full_key, path);
            }
        }
    }
}

/// 既定値を書き出す。既にファイルがあれば上書きしない。
pub fn init() -> Result<PathBuf, ConfigError> {
    let path = config_file_path()?;
    if path.exists() {
        return Err(ConfigError::AlreadyExists(path));
    }

    let dir = config_dir()?;
    std::fs::create_dir_all(&dir).map_err(|source| ConfigError::Write {
        path: path.clone(),
        source,
    })?;

    std::fs::write(&path, render_default_config()).map_err(|source| ConfigError::Write {
        path: path.clone(),
        source,
    })?;

    Ok(path)
}

const HEADER_COMMENT: &str = "\
# Humboldti Note config file — entirely optional. Every value below is
# already the built-in default; delete anything you don't want to change.
# Missing keys (including whole [keys.*] tables) just fall back silently.
#
# Precedence: --dir flag > PEN_* env vars > this file > built-in defaults.
# `pen config check` prints what's actually in effect right now.
";

const NOTES_DIR_COMMENT: &str = "# Where daily notes are written. `~` is expanded. Overridable with\n\
     # --dir or the PEN_DIR environment variable.\n";

const MERGE_WINDOW_COMMENT: &str = "# Consecutive appends within this many minutes share one time heading\n\
     # instead of starting a new one.\n";

const EDITOR_COMMENT: &str = "# Command (with any arguments) used to open a note, e.g. \"nvim\" or\n\
     # \"code --wait\". Empty (the default) falls back to $EDITOR, then \"vi\".\n\
     # GUI editors need their own \"wait for the window to close\" flag (like\n\
     # code's --wait) or pen will think you're done editing immediately.\n\
     # Overridable with the PEN_EDITOR environment variable.\n";

const KEYS_HEADER_COMMENT: &str = "\
# Keybindings for `pen cal`. Each key below the section header is the name
# of an action this version of Humboldti Note recognizes; the value is the
# list of key specs bound to it. Deleting an action's line just resets that
# one action to its built-in default.
#
# A key spec is a single character (\"h\"), a named key (\"enter\", \"esc\",
# \"left\", \"right\", \"up\", \"down\", \"tab\", \"backspace\", \"delete\", \"home\",
# \"end\", \"pageup\", \"pagedown\", \"space\"), optionally prefixed with
# modifiers (\"ctrl-a\", \"ctrl-shift-tab\"). Vim notation (\"<C-a>\") isn't
# supported.
#
# A typo'd action name is ignored with a warning. An unparseable key spec,
# or two actions in the same table claiming the same key, fails to start
# `pen cal`.
";

const SEARCH_INPUT_COMMENT: &str = "# While typing a search query. Any key not listed here is typed into\n\
     # the query as-is.\n";

const SEARCH_RESULTS_COMMENT: &str = "# While browsing search results.\n";

/// `Config::default()` を実際にシリアライズした値に、モードの並びを見ながら
/// 説明コメントを差し込む。値そのものは常に `Config::default()` 由来なので、
/// ここで二重管理になっているのはコメントの文面だけ(そして文面は既定値が
/// 変わっても書き直しを要らないよう、具体的な数値を書かないようにしてある)。
fn render_default_config() -> String {
    // `to_string_pretty` は複数要素の配列を折り返す。既定のキー割り当ては
    // どれも数個の短い文字列なので、折り返さない `to_string` の方が
    // 一覧性がよい(セクションヘッダー `[keys.calendar]` 等は pretty かどうかに
    // 関係なく出る)。
    let toml = toml::to_string(&Config::default()).expect("Config always serializes to TOML");

    let mut out = String::from(HEADER_COMMENT);
    out.push('\n');
    for line in toml.lines() {
        match line {
            _ if line.starts_with("notes_dir") => out.push_str(NOTES_DIR_COMMENT),
            _ if line.starts_with("merge_window_minutes") => out.push_str(MERGE_WINDOW_COMMENT),
            _ if line.starts_with("editor") => out.push_str(EDITOR_COMMENT),
            "[keys.calendar]" => out.push_str(KEYS_HEADER_COMMENT),
            "[keys.search_input]" => out.push_str(SEARCH_INPUT_COMMENT),
            "[keys.search_results]" => out.push_str(SEARCH_RESULTS_COMMENT),
            _ => {}
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_resolves_to_home_dir() {
        let expanded = expand_tilde("~/notes");
        assert!(expanded.is_absolute());
        assert!(expanded.ends_with("notes"));
        assert!(!expanded.starts_with("~"));
    }

    #[test]
    fn app_config_dir_appends_app_name() {
        let base = Path::new("/home/someone/.config");
        assert_eq!(
            app_config_dir(base),
            PathBuf::from("/home/someone/.config/pen")
        );
    }

    #[test]
    // Jail::expect_with のクロージャは figment::Error を返す契約で、これは
    // figment 側の型なのでこちらでは小さくできない。
    #[allow(clippy::result_large_err)]
    fn resolve_respects_precedence() {
        figment::Jail::expect_with(|jail| {
            let home = jail.directory().to_path_buf();
            jail.set_env("HOME", home.display());
            jail.set_env("XDG_CONFIG_HOME", home.join("xdg").display());
            jail.create_dir("xdg/pen")?;

            // 既定値のみ
            let config = resolve(None).map_err(|e| e.to_string())?;
            assert!(config.notes_dir.ends_with("notes"));

            // 設定ファイル > 既定値
            jail.create_file("xdg/pen/config.toml", "notes_dir = \"/from/file\"\n")?;
            let config = resolve(None).map_err(|e| e.to_string())?;
            assert_eq!(config.notes_dir, PathBuf::from("/from/file"));

            // 環境変数 > 設定ファイル
            jail.set_env("PEN_DIR", "/from/env");
            let config = resolve(None).map_err(|e| e.to_string())?;
            assert_eq!(config.notes_dir, PathBuf::from("/from/env"));

            // CLI フラグ > 環境変数
            let config = resolve(Some(PathBuf::from("/from/cli"))).map_err(|e| e.to_string())?;
            assert_eq!(config.notes_dir, PathBuf::from("/from/cli"));

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn resolve_applies_editor_precedence() {
        figment::Jail::expect_with(|jail| {
            let home = jail.directory().to_path_buf();
            jail.set_env("HOME", home.display());
            jail.set_env("XDG_CONFIG_HOME", home.join("xdg").display());
            jail.create_dir("xdg/pen")?;

            // 既定値は空文字列(未設定)。
            let config = resolve(None).map_err(|e| e.to_string())?;
            assert_eq!(config.editor, "");

            // 設定ファイル > 既定値
            jail.create_file("xdg/pen/config.toml", "editor = \"nvim\"\n")?;
            let config = resolve(None).map_err(|e| e.to_string())?;
            assert_eq!(config.editor, "nvim");

            // 環境変数(PEN_EDITOR) > 設定ファイル
            jail.set_env("PEN_EDITOR", "code --wait");
            let config = resolve(None).map_err(|e| e.to_string())?;
            assert_eq!(config.editor, "code --wait");

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn warn_unknown_keys_detects_unrecognized_top_level_key() {
        figment::Jail::expect_with(|jail| {
            let home = jail.directory().to_path_buf();
            jail.set_env("HOME", home.display());
            jail.set_env("XDG_CONFIG_HOME", home.join("xdg").display());
            jail.create_dir("xdg/pen")?;
            jail.create_file("xdg/pen/config.toml", "notes_dir = \"/x\"\nbogus_key = 1\n")?;

            // eprintln! の出力先までは検証しないが、エラーにはならないこと
            // (=起動を止めないこと)を確認する。
            warn_unknown_keys().map_err(|e| e.to_string())?;
            Ok(())
        });
    }

    #[test]
    #[allow(clippy::result_large_err)]
    fn warn_unknown_keys_detects_a_typo_inside_a_keys_section() {
        figment::Jail::expect_with(|jail| {
            let home = jail.directory().to_path_buf();
            jail.set_env("HOME", home.display());
            jail.set_env("XDG_CONFIG_HOME", home.join("xdg").display());
            jail.create_dir("xdg/pen")?;
            jail.create_file(
                "xdg/pen/config.toml",
                "notes_dir = \"/x\"\n[keys.calendar]\nnxet_day = [\"n\"]\n",
            )?;

            // ネストした [keys.calendar] の中のタイポも、起動は止めずに検出できること。
            warn_unknown_keys().map_err(|e| e.to_string())?;
            Ok(())
        });
    }

    #[test]
    fn default_config_serializes_the_documented_calendar_binding() {
        let toml = toml::to_string_pretty(&Config::default()).unwrap();
        let parsed: toml::Value = toml.parse().unwrap();

        let next_day = parsed["keys"]["calendar"]["next_day"]
            .as_array()
            .expect("keys.calendar.next_day should be an array");
        let specs: Vec<&str> = next_day.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(specs, ["l", "right"]);
    }

    #[test]
    fn rendered_default_config_is_commented_but_still_parses_to_the_real_defaults() {
        let rendered = render_default_config();

        // コメントが添えられていること(でなければこのテストを書く意味がない)。
        assert!(rendered.contains("# Humboldti Note config file"));
        assert!(rendered.contains("# Where daily notes are written"));
        assert!(rendered.contains("# Command (with any arguments) used to open a note"));
        assert!(rendered.contains("# Keybindings for `pen cal`"));

        // コメントは TOML のコメント構文として妥当で、パースすると実際の
        // 既定値になること。値は常に `Config::default()` からシリアライズ
        // しているので、これが崩れることはあり得ない — 崩れたらそれは
        // `render_default_config` 自体のバグ(コメントを値の途中に差し込んで
        // しまった等)を意味する。`notes_dir` だけは他のテストが `Jail` で
        // 並行して `$HOME` を書き換えうるので、緩く検証する
        // (`resolve_respects_precedence` と同じ理由)。
        let parsed: Config = toml::from_str(&rendered).expect("rendered config must be valid TOML");
        assert!(parsed.notes_dir.ends_with("notes"));
        assert_eq!(parsed.merge_window_minutes, 30);
        assert_eq!(parsed.editor, "");
        assert_eq!(parsed.keys, KeysConfig::default());
    }
}
