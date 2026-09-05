use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::KeysConfig;
use crate::keys::{self, KeyParseError};

/// 今どの画面にいるか。キー解決はモードごとに変わる。
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Calendar,
    /// 検索クエリを入力中。文字キーはナビゲーションではなく入力として扱う。
    SearchInput,
    /// 検索結果の一覧を選んでいる。
    SearchResults,
}

/// 意図で命名する。キー割り当てが変わっても名前は変えない
/// (設定ファイルからアクション名を参照する利用者がいる)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PrevDay,
    NextDay,
    PrevWeek,
    NextWeek,
    PrevMonth,
    NextMonth,
    PrevYear,
    NextYear,
    Open,
    Quit,
    EnterSearch,
    InputChar(char),
    Backspace,
    Confirm,
    Cancel,
    NextResult,
    PrevResult,
}

/// 設定可能なアクションと既定のキー割り当て。TOML のキー名(`prev_day` など)
/// と `Action` の対応もここが唯一の情報源で、`KeysConfig::default()` は
/// これを呼ぶだけにする(既定値を二重に書かない)。
const CALENDAR_ACTIONS: &[(&str, Action, &[&str])] = &[
    ("prev_day", Action::PrevDay, &["h", "left"]),
    ("next_day", Action::NextDay, &["l", "right"]),
    ("prev_week", Action::PrevWeek, &["k", "up"]),
    ("next_week", Action::NextWeek, &["j", "down"]),
    ("prev_month", Action::PrevMonth, &["["]),
    ("next_month", Action::NextMonth, &["]"]),
    ("prev_year", Action::PrevYear, &["{"]),
    ("next_year", Action::NextYear, &["}"]),
    ("open", Action::Open, &["enter"]),
    ("quit", Action::Quit, &["q", "esc"]),
    ("enter_search", Action::EnterSearch, &["/"]),
];

/// `InputChar` は「割り当てられていないキーを文字入力として扱う」という
/// フォールバックであって、設定可能なアクションではないのでここには出さない。
const SEARCH_INPUT_ACTIONS: &[(&str, Action, &[&str])] = &[
    ("confirm", Action::Confirm, &["enter"]),
    ("cancel", Action::Cancel, &["esc"]),
    ("backspace", Action::Backspace, &["backspace"]),
];

const SEARCH_RESULTS_ACTIONS: &[(&str, Action, &[&str])] = &[
    ("next_result", Action::NextResult, &["j", "down"]),
    ("prev_result", Action::PrevResult, &["k", "up"]),
    ("confirm", Action::Confirm, &["enter"]),
    ("cancel", Action::Cancel, &["q", "esc"]),
];

fn actions_for_mode(mode: Mode) -> &'static [(&'static str, Action, &'static [&'static str])] {
    match mode {
        Mode::Calendar => CALENDAR_ACTIONS,
        Mode::SearchInput => SEARCH_INPUT_ACTIONS,
        Mode::SearchResults => SEARCH_RESULTS_ACTIONS,
    }
}

/// `Config::default()` が使う、モードごとの既定キー割り当て。
pub fn default_bindings(mode: Mode) -> BTreeMap<String, Vec<String>> {
    actions_for_mode(mode)
        .iter()
        .map(|(name, _, keys)| {
            (
                (*name).to_string(),
                keys.iter().map(|k| (*k).to_string()).collect(),
            )
        })
        .collect()
}

#[derive(thiserror::Error, Debug)]
pub enum KeyMapError {
    #[error("invalid key `{key}` bound to `{action}` in [keys.{mode}]: {source}")]
    InvalidKey {
        mode: &'static str,
        action: String,
        key: String,
        #[source]
        source: KeyParseError,
    },
    #[error("key `{key}` in [keys.{mode}] is bound to both `{first}` and `{second}`")]
    Conflict {
        mode: &'static str,
        key: String,
        first: String,
        second: String,
    },
}

/// 設定から組み立てた、モードごとのキー→アクション表。
pub struct KeyMap {
    calendar: HashMap<(KeyCode, KeyModifiers), Action>,
    search_input: HashMap<(KeyCode, KeyModifiers), Action>,
    search_results: HashMap<(KeyCode, KeyModifiers), Action>,
}

impl KeyMap {
    pub fn from_config(cfg: &KeysConfig) -> Result<Self, KeyMapError> {
        Ok(KeyMap {
            calendar: build_mode_map("calendar", Mode::Calendar, &cfg.calendar)?,
            search_input: build_mode_map("search_input", Mode::SearchInput, &cfg.search_input)?,
            search_results: build_mode_map(
                "search_results",
                Mode::SearchResults,
                &cfg.search_results,
            )?,
        })
    }

    fn table(&self, mode: Mode) -> &HashMap<(KeyCode, KeyModifiers), Action> {
        match mode {
            Mode::Calendar => &self.calendar,
            Mode::SearchInput => &self.search_input,
            Mode::SearchResults => &self.search_results,
        }
    }
}

/// 設定ファイルに知らないアクション名があってもここでは無視する
/// (`config::warn_unknown_keys` が別途 stderr に警告する)。ここで検出するのは
/// キー仕様のパースエラーと、同じキーへの重複割り当てだけ。
fn build_mode_map(
    mode: &'static str,
    lookup_mode: Mode,
    configured: &BTreeMap<String, Vec<String>>,
) -> Result<HashMap<(KeyCode, KeyModifiers), Action>, KeyMapError> {
    let mut map = HashMap::new();
    let mut owners: HashMap<(KeyCode, KeyModifiers), &str> = HashMap::new();

    for (name, action, _) in actions_for_mode(lookup_mode) {
        let Some(specs) = configured.get(*name) else {
            continue;
        };
        for spec in specs {
            let (code, modifiers) =
                keys::parse(spec).map_err(|source| KeyMapError::InvalidKey {
                    mode,
                    action: (*name).to_string(),
                    key: spec.clone(),
                    source,
                })?;
            if let Some(&owner) = owners.get(&(code, modifiers))
                && owner != *name
            {
                return Err(KeyMapError::Conflict {
                    mode,
                    key: spec.clone(),
                    first: owner.to_string(),
                    second: (*name).to_string(),
                });
            }
            owners.insert((code, modifiers), name);
            map.insert((code, modifiers), *action);
        }
    }

    Ok(map)
}

/// `KeyEvent` を直接 match するのはこの関数だけにする。呼び出し側は
/// `Action` だけを見て、キーそのものを知らなくてよいようにする。
pub fn resolve(keymap: &KeyMap, key: KeyEvent, mode: Mode) -> Option<Action> {
    if let Some(&action) = keymap.table(mode).get(&(key.code, key.modifiers)) {
        return Some(action);
    }
    match mode {
        // 割り当てられていない文字キーは、そのまま検索クエリの入力として扱う。
        Mode::SearchInput => match key.code {
            KeyCode::Char(c) => Some(Action::InputChar(c)),
            _ => None,
        },
        Mode::Calendar | Mode::SearchResults => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn default_keymap() -> KeyMap {
        KeyMap::from_config(&KeysConfig::default()).unwrap()
    }

    #[test]
    fn hjkl_and_arrows_resolve_to_the_same_actions() {
        let m = default_keymap();
        assert_eq!(
            resolve(&m, key(KeyCode::Char('h')), Mode::Calendar),
            resolve(&m, key(KeyCode::Left), Mode::Calendar)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('l')), Mode::Calendar),
            resolve(&m, key(KeyCode::Right), Mode::Calendar)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('k')), Mode::Calendar),
            resolve(&m, key(KeyCode::Up), Mode::Calendar)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('j')), Mode::Calendar),
            resolve(&m, key(KeyCode::Down), Mode::Calendar)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('h')), Mode::Calendar),
            Some(Action::PrevDay)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('l')), Mode::Calendar),
            Some(Action::NextDay)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('k')), Mode::Calendar),
            Some(Action::PrevWeek)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('j')), Mode::Calendar),
            Some(Action::NextWeek)
        );
    }

    #[test]
    fn enter_opens_and_q_or_esc_quits() {
        let m = default_keymap();
        assert_eq!(
            resolve(&m, key(KeyCode::Enter), Mode::Calendar),
            Some(Action::Open)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('q')), Mode::Calendar),
            Some(Action::Quit)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Esc), Mode::Calendar),
            Some(Action::Quit)
        );
    }

    #[test]
    fn brackets_jump_by_month_and_year() {
        let m = default_keymap();
        assert_eq!(
            resolve(&m, key(KeyCode::Char('[')), Mode::Calendar),
            Some(Action::PrevMonth)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char(']')), Mode::Calendar),
            Some(Action::NextMonth)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('{')), Mode::Calendar),
            Some(Action::PrevYear)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('}')), Mode::Calendar),
            Some(Action::NextYear)
        );
    }

    #[test]
    fn unknown_key_resolves_to_none() {
        let m = default_keymap();
        assert_eq!(resolve(&m, key(KeyCode::Char('x')), Mode::Calendar), None);
    }

    #[test]
    fn slash_enters_search_from_calendar() {
        let m = default_keymap();
        assert_eq!(
            resolve(&m, key(KeyCode::Char('/')), Mode::Calendar),
            Some(Action::EnterSearch)
        );
    }

    #[test]
    fn search_input_treats_letters_as_text() {
        let m = default_keymap();
        assert_eq!(
            resolve(&m, key(KeyCode::Char('j')), Mode::SearchInput),
            Some(Action::InputChar('j'))
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('あ')), Mode::SearchInput),
            Some(Action::InputChar('あ'))
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Backspace), Mode::SearchInput),
            Some(Action::Backspace)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Enter), Mode::SearchInput),
            Some(Action::Confirm)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Esc), Mode::SearchInput),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn search_results_uses_jk_for_navigation_not_text() {
        let m = default_keymap();
        assert_eq!(
            resolve(&m, key(KeyCode::Char('j')), Mode::SearchResults),
            Some(Action::NextResult)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('k')), Mode::SearchResults),
            Some(Action::PrevResult)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Enter), Mode::SearchResults),
            Some(Action::Confirm)
        );
        assert_eq!(
            resolve(&m, key(KeyCode::Char('q')), Mode::SearchResults),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn custom_binding_overrides_the_default() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("next_day".to_string(), vec!["n".to_string()]);
        let m = KeyMap::from_config(&cfg).unwrap();

        assert_eq!(
            resolve(&m, key(KeyCode::Char('n')), Mode::Calendar),
            Some(Action::NextDay)
        );
        // 既定の `l` は上書きされて消える。
        assert_eq!(resolve(&m, key(KeyCode::Char('l')), Mode::Calendar), None);
    }

    #[test]
    fn conflicting_bindings_in_the_same_mode_are_a_startup_error() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("next_day".to_string(), vec!["h".to_string()]);

        assert!(matches!(
            KeyMap::from_config(&cfg),
            Err(KeyMapError::Conflict { .. })
        ));
    }

    #[test]
    fn invalid_key_spec_is_a_startup_error() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("next_day".to_string(), vec!["ctrl-".to_string()]);

        assert!(matches!(
            KeyMap::from_config(&cfg),
            Err(KeyMapError::InvalidKey { .. })
        ));
    }

    #[test]
    fn unknown_action_names_in_config_are_ignored_here() {
        // タイポの警告は `config::warn_unknown_keys` の役目で、KeyMap の
        // 構築自体は失敗させない。
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("nxet_day".to_string(), vec!["n".to_string()]);

        assert!(KeyMap::from_config(&cfg).is_ok());
    }
}
