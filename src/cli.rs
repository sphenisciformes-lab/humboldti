use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A minimal daily markdown note-taking tool for your terminal.
///
/// Shell metacharacters in your note text (`&`, `|`, `>`, `[`, `]`, `*`, `?`, `~`, ...)
/// are handled by your shell before pen ever sees them; pen cannot protect
/// you from that.
///
/// Quote your note, or pipe it in instead:
///
///   pen "- [ ] buy milk"
///
///   echo "- [ ] buy milk" | pen -
///
/// `-t`/`--todo` is a shortcut for the checklist syntax above:
///
///   pen -t "buy milk"
///
/// If your note would otherwise be parsed as a subcommand (for example it starts
/// with the word "config", "open", "cal", "search", "context", or "mcp"), put
/// `--` before it:
///
///   pen -- config the server
#[derive(Parser)]
#[command(name = "pen", version)]
pub struct Cli {
    /// Override notes_dir for this invocation.
    #[arg(long, global = true)]
    pub dir: Option<PathBuf>,

    /// Print machine-readable JSON instead of plain text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Format the text as an unchecked checklist item (`- [ ] <text>`)
    /// instead of appending it as-is. No effect if the text already starts
    /// with `- [`.
    #[arg(short = 't', long)]
    pub todo: bool,

    /// Text to append to today's note. Reads from stdin instead if the only
    /// argument is `-`. Use `--` before text that would otherwise be parsed
    /// as a subcommand.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub text: Vec<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Inspect or create the config file.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Open a day's note in $EDITOR (or `vi` if unset). Defaults to today.
    Open {
        /// Date to open, e.g. 2026-08-29. Defaults to today when omitted.
        date: Option<chrono::NaiveDate>,
    },
    /// Browse notes in a calendar view.
    ///
    /// hjkl/arrows: move by day/week
    ///
    /// [ ]: jump by month
    ///
    /// { }: jump by year
    ///
    /// /: search, Enter: open, q/Esc: quit
    Cal,
    /// Search notes. The query is a regular expression (case-insensitive).
    Search { query: Vec<String> },
    /// Print recent notes in a form meant for feeding to an LLM.
    Context {
        /// How far back to go, e.g. `7d` or `2w`.
        #[arg(long, default_value = "7d")]
        since: Since,
        /// Approximate token budget (a rough character-based estimate, not an
        /// exact count for any specific model's tokenizer). Whole days are
        /// dropped from the oldest end once the budget would be exceeded.
        #[arg(long, default_value_t = 4000)]
        max_tokens: usize,
    },
    /// Run an MCP server (stdio transport) exposing search_notes, read_note,
    /// and append_note to AI agents.
    Mcp,
}

/// A duration expressed as `<number>d` (days) or `<number>w` (weeks).
/// Notes are one file per day, so finer-grained units like hours wouldn't
/// change which files are selected.
#[derive(Debug, Clone, Copy)]
pub struct Since(pub u32);

impl std::str::FromStr for Since {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("invalid duration `{s}`, expected e.g. `7d` or `2w`");
        // バイト単位の split_at だと非 ASCII な末尾文字で文字境界を割って
        // panic しうるので、char 単位で扱う。
        let mut chars = s.chars();
        let unit = chars.next_back().ok_or_else(invalid)?;
        let n: u32 = chars.as_str().parse().map_err(|_| invalid())?;
        match unit {
            'd' => Ok(Since(n)),
            'w' => Ok(Since(n * 7)),
            _ => Err(invalid()),
        }
    }
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Print the resolved path of the config file.
    Path,
    /// Write the default config file. Fails if it already exists.
    Init,
    /// Load the config (applying the same precedence as normal commands) and print it.
    Check,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn since_parses_days_and_weeks() {
        assert_eq!(Since::from_str("7d").unwrap().0, 7);
        assert_eq!(Since::from_str("2w").unwrap().0, 14);
        assert_eq!(Since::from_str("0d").unwrap().0, 0);
    }

    #[test]
    fn since_rejects_invalid_input() {
        assert!(Since::from_str("").is_err());
        assert!(Since::from_str("d").is_err());
        assert!(Since::from_str("7").is_err());
        assert!(Since::from_str("7h").is_err());
        assert!(Since::from_str("abcd").is_err());
        // 非 ASCII な末尾文字でも panic せずエラーになること。
        assert!(Since::from_str("7日").is_err());
    }

    // `--version`/`-V` は自由記述テキストの `allow_hyphen_values` に
    // 飲み込まれず、clap 側で処理されなければならない
    // (さもないと今日のメモに `--version` という行がそのまま追記される)。
    #[test]
    fn version_flag_is_handled_by_clap_not_captured_as_text() {
        use clap::error::ErrorKind;

        let err = Cli::try_parse_from(["pen", "--version"]).err().unwrap();
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);

        let err = Cli::try_parse_from(["pen", "-V"]).err().unwrap();
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    }

    #[test]
    fn hyphen_prefixed_note_text_still_parses_as_text() {
        let cli = Cli::try_parse_from(["pen", "-500円使った"]).unwrap();
        assert_eq!(cli.text, vec!["-500円使った"]);
    }
}
