use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::KeysConfig;
use crate::keys::{self, KeyParseError};

/// 今どの画面にいるか。キー解決はモードごとに変わる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Calendar,
    /// 検索クエリを入力中。文字キーはナビゲーションではなく入力として扱う。
    SearchInput,
    /// 検索結果の一覧を選んでいる。
    SearchResults,
}

/// 意図で命名する。キー割り当てが変わっても名前は変えない
/// (設定ファイルからアクション名を参照する利用者がいる)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    #[error("key `{key}` bound to `{action}` in [keys.{mode}] is reserved: ctrl-c always quits")]
    Reserved {
        mode: &'static str,
        action: String,
        key: String,
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
    calendar: ModeMap,
    search_input: ModeMap,
    search_results: ModeMap,
}

/// 1つのモードの割り当て。`first_key` は案内表示用で、各アクションに
/// 設定で最初に書かれたキー仕様(書かれた通りの文字列)を持つ。
struct ModeMap {
    keys: HashMap<(KeyCode, KeyModifiers), Action>,
    first_key: HashMap<Action, String>,
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

    fn mode_map(&self, mode: Mode) -> &ModeMap {
        match mode {
            Mode::Calendar => &self.calendar,
            Mode::SearchInput => &self.search_input,
            Mode::SearchResults => &self.search_results,
        }
    }

    fn table(&self, mode: Mode) -> &HashMap<(KeyCode, KeyModifiers), Action> {
        &self.mode_map(mode).keys
    }

    /// 画面に出すキーの案内(`"h l: day"` のような項目の並び)。キーは
    /// 実際の割り当てから引くので、設定で割り当てを変えても案内がずれない。
    /// 幅を取りすぎないよう、各アクションには最初に割り当てたキーだけを出す。
    /// キーが1つも割り当てられていないアクションは出さない。
    pub fn help_items(&self, mode: Mode) -> Vec<String> {
        let first_key = &self.mode_map(mode).first_key;
        help_for_mode(mode)
            .iter()
            .filter_map(|(label, actions)| {
                let keys: Vec<String> = actions
                    .iter()
                    .filter_map(|action| first_key.get(action))
                    .map(|spec| display_key(spec))
                    .collect();
                (!keys.is_empty()).then(|| format!("{}: {label}", keys.join(" ")))
            })
            .collect()
    }
}

/// 案内に並べる項目。前後の移動のように対になるアクションは1項目にまとめる。
/// ここには何を並べるかだけを書き、キーそのものは書かない。
const CALENDAR_HELP: &[(&str, &[Action])] = &[
    ("day", &[Action::PrevDay, Action::NextDay]),
    ("week", &[Action::PrevWeek, Action::NextWeek]),
    ("month", &[Action::PrevMonth, Action::NextMonth]),
    ("year", &[Action::PrevYear, Action::NextYear]),
    ("search", &[Action::EnterSearch]),
    ("open", &[Action::Open]),
    ("quit", &[Action::Quit]),
];

const SEARCH_INPUT_HELP: &[(&str, &[Action])] = &[
    ("search", &[Action::Confirm]),
    ("cancel", &[Action::Cancel]),
];

const SEARCH_RESULTS_HELP: &[(&str, &[Action])] = &[
    ("move", &[Action::NextResult, Action::PrevResult]),
    ("open", &[Action::Confirm]),
    ("back", &[Action::Cancel]),
];

fn help_for_mode(mode: Mode) -> &'static [(&'static str, &'static [Action])] {
    match mode {
        Mode::Calendar => CALENDAR_HELP,
        Mode::SearchInput => SEARCH_INPUT_HELP,
        Mode::SearchResults => SEARCH_RESULTS_HELP,
    }
}

/// キー仕様を案内用に整える。`enter`・`ctrl-a` のような名前付きのキーと
/// 修飾キーは先頭を大文字に(`Enter`、`Ctrl-a`)し、1文字のキーはそのまま
/// 出す(`G` と `g` は別のキーなので大文字小文字を変えられない)。
fn display_key(spec: &str) -> String {
    spec.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match (chars.next(), chars.clone().next()) {
                (Some(first), Some(_)) => first.to_uppercase().chain(chars).collect(),
                _ => part.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// 設定ファイルに知らないアクション名があってもここでは無視する
/// (`config::warn_unknown_keys` が別途 stderr に警告する)。ここで検出するのは
/// キー仕様のパースエラーと、同じキーへの重複割り当てだけ。
fn build_mode_map(
    mode: &'static str,
    lookup_mode: Mode,
    configured: &BTreeMap<String, Vec<String>>,
) -> Result<ModeMap, KeyMapError> {
    let mut map = HashMap::new();
    let mut first_key = HashMap::new();
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
            if (code, modifiers) == QUIT_KEY {
                return Err(KeyMapError::Reserved {
                    mode,
                    action: (*name).to_string(),
                    key: spec.clone(),
                });
            }
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
            first_key.entry(*action).or_insert_with(|| spec.clone());
        }
    }

    Ok(ModeMap {
        keys: map,
        first_key,
    })
}

/// raw mode では端末が Ctrl-C を SIGINT に変えないので、自前で扱わないと
/// Ctrl-C では抜けられない。「とにかく抜ける」キーとして端末の慣習どおりに
/// 動くよう、割り当て変更の対象外にして、どの画面でも終了にする。
const QUIT_KEY: (KeyCode, KeyModifiers) = (KeyCode::Char('c'), KeyModifiers::CONTROL);

/// `KeyEvent` を直接 match するのはこの関数だけにする。呼び出し側は
/// `Action` だけを見て、キーそのものを知らなくてよいようにする。
pub fn resolve(keymap: &KeyMap, key: KeyEvent, mode: Mode) -> Option<Action> {
    let normalized = keys::normalize(key.code, key.modifiers);
    if normalized == QUIT_KEY {
        return Some(Action::Quit);
    }
    if let Some(&action) = keymap.table(mode).get(&normalized) {
        return Some(action);
    }
    match mode {
        // 割り当てられていない文字キーは、そのまま検索クエリの入力として扱う。
        // Ctrl や Alt 付きのキーは文字の入力ではないので、クエリに入れない。
        Mode::SearchInput => match key.code {
            KeyCode::Char(c) if (key.modifiers - KeyModifiers::SHIFT).is_empty() => {
                Some(Action::InputChar(c))
            }
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

    // crossterm は大文字を SHIFT 付きで送ってくる(`Char('G')` + SHIFT)。
    #[test]
    fn uppercase_bindings_match_the_shifted_event_crossterm_sends() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("next_month".to_string(), vec!["N".to_string()]);
        cfg.calendar
            .insert("prev_month".to_string(), vec!["shift-p".to_string()]);
        let m = KeyMap::from_config(&cfg).unwrap();
        let shifted = |c| KeyEvent {
            modifiers: KeyModifiers::SHIFT,
            ..key(KeyCode::Char(c))
        };

        assert_eq!(
            resolve(&m, shifted('N'), Mode::Calendar),
            Some(Action::NextMonth)
        );
        assert_eq!(
            resolve(&m, shifted('P'), Mode::Calendar),
            Some(Action::PrevMonth)
        );
        // 小文字の割り当てには影響しない。
        assert_eq!(
            resolve(&m, key(KeyCode::Char('h')), Mode::Calendar),
            Some(Action::PrevDay)
        );
    }

    #[test]
    fn uppercase_and_shift_forms_of_one_key_conflict() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("next_month".to_string(), vec!["G".to_string()]);
        cfg.calendar
            .insert("prev_month".to_string(), vec!["shift-g".to_string()]);

        assert!(matches!(
            KeyMap::from_config(&cfg),
            Err(KeyMapError::Conflict { .. })
        ));
    }

    #[test]
    fn help_items_show_the_default_keys() {
        let m = default_keymap();
        assert_eq!(
            m.help_items(Mode::Calendar),
            [
                "h l: day",
                "k j: week",
                "[ ]: month",
                "{ }: year",
                "/: search",
                "Enter: open",
                "q: quit"
            ]
        );
        assert_eq!(
            m.help_items(Mode::SearchInput),
            ["Enter: search", "Esc: cancel"]
        );
        assert_eq!(
            m.help_items(Mode::SearchResults),
            ["j k: move", "Enter: open", "q: back"]
        );
    }

    #[test]
    fn help_items_follow_rebound_and_unbound_keys() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("next_day".to_string(), vec!["n".to_string()]);
        cfg.calendar.insert("open".to_string(), vec![]);
        cfg.calendar
            .insert("quit".to_string(), vec!["ctrl-q".to_string()]);
        let m = KeyMap::from_config(&cfg).unwrap();

        let items = m.help_items(Mode::Calendar);
        assert!(items.contains(&"h n: day".to_string()));
        assert!(items.contains(&"Ctrl-q: quit".to_string()));
        // キーが1つも無いアクションは案内に出さない。
        assert!(!items.iter().any(|item| item.ends_with(": open")));
    }

    #[test]
    fn display_key_capitalizes_names_but_not_single_characters() {
        assert_eq!(display_key("enter"), "Enter");
        assert_eq!(display_key("ctrl-shift-tab"), "Ctrl-Shift-Tab");
        assert_eq!(display_key("ctrl-a"), "Ctrl-a");
        assert_eq!(display_key("G"), "G");
        assert_eq!(display_key("g"), "g");
        assert_eq!(display_key("/"), "/");
    }

    #[test]
    fn ctrl_c_quits_from_every_screen() {
        let m = default_keymap();
        let ctrl_c = KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char('c'))
        };
        for mode in [Mode::Calendar, Mode::SearchInput, Mode::SearchResults] {
            assert_eq!(resolve(&m, ctrl_c, mode), Some(Action::Quit));
        }
    }

    #[test]
    fn ctrl_and_alt_keys_are_not_typed_into_the_search_query() {
        let m = default_keymap();
        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            let event = KeyEvent {
                modifiers,
                ..key(KeyCode::Char('x'))
            };
            assert_eq!(resolve(&m, event, Mode::SearchInput), None);
        }
        // Shift 付きの文字(大文字)は普通に入力できる。
        let shifted = KeyEvent {
            modifiers: KeyModifiers::SHIFT,
            ..key(KeyCode::Char('X'))
        };
        assert_eq!(
            resolve(&m, shifted, Mode::SearchInput),
            Some(Action::InputChar('X'))
        );
    }

    #[test]
    fn binding_ctrl_c_is_a_startup_error() {
        let mut cfg = KeysConfig::default();
        cfg.calendar
            .insert("open".to_string(), vec!["ctrl-c".to_string()]);

        assert!(matches!(
            KeyMap::from_config(&cfg),
            Err(KeyMapError::Reserved { .. })
        ));
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
