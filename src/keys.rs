use crossterm::event::{KeyCode, KeyModifiers};

#[derive(thiserror::Error, Debug)]
pub enum KeyParseError {
    #[error("key spec is empty")]
    Empty,
    #[error("unknown modifier `{0}`, expected `ctrl`, `alt`, or `shift`")]
    UnknownModifier(String),
    #[error("unknown key name `{0}`")]
    UnknownKey(String),
}

/// ケバブ小文字の仕様(`h`、`ctrl-a`、`enter` など)を
/// `(KeyCode, KeyModifiers)` に変換する。`C-a` や `<C-a>` のような
/// vim 記法は受け付けない(`docs/DESIGN.md` の「名前」節の通り)。
pub fn parse(spec: &str) -> Result<(KeyCode, KeyModifiers), KeyParseError> {
    if spec.is_empty() {
        return Err(KeyParseError::Empty);
    }

    let mut parts: Vec<&str> = spec.split('-').collect();
    let base = parts.pop().filter(|b| !b.is_empty());

    let mut modifiers = KeyModifiers::NONE;
    for part in &parts {
        modifiers |= match *part {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            other => return Err(KeyParseError::UnknownModifier(other.to_string())),
        };
    }

    let base = base.ok_or_else(|| KeyParseError::UnknownKey(spec.to_string()))?;
    let code = named_key(base).ok_or_else(|| KeyParseError::UnknownKey(base.to_string()))?;

    Ok((code, modifiers))
}

fn named_key(name: &str) -> Option<KeyCode> {
    let named = match name {
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        _ => {
            let mut chars = name.chars();
            let c = chars.next()?;
            return if chars.next().is_none() {
                Some(KeyCode::Char(c))
            } else {
                None
            };
        }
    };
    Some(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_bare_char() {
        assert_eq!(
            parse("h").unwrap(),
            (KeyCode::Char('h'), KeyModifiers::NONE)
        );
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(parse("left").unwrap(), (KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(
            parse("enter").unwrap(),
            (KeyCode::Enter, KeyModifiers::NONE)
        );
    }

    #[test]
    fn parses_a_modifier_prefix() {
        assert_eq!(
            parse("ctrl-a").unwrap(),
            (KeyCode::Char('a'), KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn parses_stacked_modifiers() {
        assert_eq!(
            parse("ctrl-shift-tab").unwrap(),
            (KeyCode::Tab, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn parses_punctuation_as_a_char() {
        assert_eq!(
            parse("/").unwrap(),
            (KeyCode::Char('/'), KeyModifiers::NONE)
        );
        assert_eq!(
            parse("[").unwrap(),
            (KeyCode::Char('['), KeyModifiers::NONE)
        );
    }

    #[test]
    fn rejects_empty_spec() {
        assert!(matches!(parse(""), Err(KeyParseError::Empty)));
    }

    #[test]
    fn rejects_unknown_modifier() {
        assert!(matches!(
            parse("meta-a"),
            Err(KeyParseError::UnknownModifier(m)) if m == "meta"
        ));
    }

    #[test]
    fn rejects_multi_char_key_name() {
        assert!(matches!(parse("xyz"), Err(KeyParseError::UnknownKey(_))));
    }
}
