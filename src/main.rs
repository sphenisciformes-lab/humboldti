use std::io::Read;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use humboldti_note::action::{self, Action, Mode};
use humboldti_note::cli::{Cli, Command, ConfigAction, Since};
use humboldti_note::config::{self, Config};
use humboldti_note::mcp::PenMcp;
use humboldti_note::notes;
use humboldti_note::ui::calendar::{self, CalendarRequest, CalendarState};
use humboldti_note::ui::search::{self, SearchState};
use rmcp::ServiceExt;
use serde::Serialize;

#[derive(Serialize)]
struct PathOutput {
    path: PathBuf,
}

#[derive(Serialize)]
struct CheckOutput {
    notes_dir: PathBuf,
    merge_window_minutes: u32,
    editor: String,
}

#[derive(Serialize)]
struct AppendOutput {
    path: PathBuf,
    heading: String,
    merged: bool,
}

#[derive(Serialize)]
struct SearchHitOutput {
    path: PathBuf,
    date: String,
    line_number: usize,
    line: String,
}

#[derive(Serialize)]
struct SearchOutput {
    hits: Vec<SearchHitOutput>,
}

#[derive(Serialize)]
struct ContextDayOutput {
    date: String,
    content: String,
}

#[derive(Serialize)]
struct ContextOutput {
    since_days: u32,
    max_tokens: usize,
    estimated_tokens: usize,
    days: Vec<ContextDayOutput>,
}

fn output<T: Serialize>(json: bool, value: T, plain: String) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("{plain}");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let json = cli.json;

    config::warn_unknown_keys()?;
    let cfg = config::resolve(cli.dir)?;

    match cli.command {
        Some(Command::Config { action }) => run_config_command(json, &cfg, action),
        Some(Command::Open { date }) => run_open(json, &cfg, date),
        Some(Command::Cal) => run_cal(&cfg),
        Some(Command::Search { query }) => run_search(json, &cfg, &query),
        Some(Command::Context { since, max_tokens }) => run_context(json, &cfg, since, max_tokens),
        Some(Command::Mcp) => run_mcp(&cfg),
        None if cli.text.is_empty() => run_open(json, &cfg, None),
        None => run_append(json, &cfg, &cli.text, cli.todo),
    }
}

fn run_config_command(json: bool, cfg: &Config, action: ConfigAction) -> anyhow::Result<()> {
    match action {
        ConfigAction::Path => {
            let path = config::config_file_path()?;
            let plain = path.display().to_string();
            output(json, PathOutput { path }, plain)
        }
        ConfigAction::Init => {
            let path = config::init()?;
            let plain = path.display().to_string();
            output(json, PathOutput { path }, plain)
        }
        ConfigAction::Check => {
            let plain = format!(
                "notes_dir = {}\nmerge_window_minutes = {}\neditor = {:?}",
                cfg.notes_dir.display(),
                cfg.merge_window_minutes,
                cfg.editor
            );
            output(
                json,
                CheckOutput {
                    notes_dir: cfg.notes_dir.clone(),
                    merge_window_minutes: cfg.merge_window_minutes,
                    editor: cfg.editor.clone(),
                },
                plain,
            )
        }
    }
}

fn run_open(json: bool, cfg: &Config, date: Option<NaiveDate>) -> anyhow::Result<()> {
    let date = date.unwrap_or_else(|| Local::now().date_naive());
    let path = notes::open_in_editor(&cfg.notes_dir, date, cfg.merge_window_minutes, &cfg.editor)?;
    let plain = path.display().to_string();
    output(json, PathOutput { path }, plain)
}

fn run_append(json: bool, cfg: &Config, text_args: &[String], todo: bool) -> anyhow::Result<()> {
    let text = if text_args.len() == 1 && text_args[0] == "-" {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        input
    } else {
        text_args.join(" ")
    };
    let text = if todo { notes::as_todo(&text) } else { text };

    let outcome = notes::append(
        &cfg.notes_dir,
        &text,
        cfg.merge_window_minutes,
        Local::now(),
    )?;
    let plain = format!(
        "{} ({})",
        outcome.path.display(),
        if outcome.merged {
            "merged"
        } else {
            "new heading"
        }
    );
    output(
        json,
        AppendOutput {
            path: outcome.path,
            heading: outcome.heading,
            merged: outcome.merged,
        },
        plain,
    )
}

fn run_search(json: bool, cfg: &Config, query_args: &[String]) -> anyhow::Result<()> {
    let pattern = query_args.join(" ");
    let hits = notes::search(&cfg.notes_dir, &pattern)?;

    let plain = if hits.is_empty() {
        "no matches".to_string()
    } else {
        hits.iter()
            .map(|hit| format!("{}:{}: {}", hit.path.display(), hit.line_number, hit.line))
            .collect::<Vec<_>>()
            .join("\n")
    };
    output(
        json,
        SearchOutput {
            hits: hits
                .into_iter()
                .map(|hit| SearchHitOutput {
                    path: hit.path,
                    date: hit.date.format("%Y-%m-%d").to_string(),
                    line_number: hit.line_number,
                    line: hit.line,
                })
                .collect(),
        },
        plain,
    )
}

fn run_context(json: bool, cfg: &Config, since: Since, max_tokens: usize) -> anyhow::Result<()> {
    let today = Local::now().date_naive();
    let out = notes::context(&cfg.notes_dir, since.0, max_tokens, today);

    let plain = if out.days.is_empty() {
        "no notes in range".to_string()
    } else {
        let mut sections: Vec<String> = out
            .days
            .iter()
            .map(|(date, content)| format!("# {date}\n{content}"))
            .collect();
        sections.push(format!(
            "<!-- estimated tokens: {} / budget: {} -->",
            out.estimated_tokens, max_tokens
        ));
        sections.join("\n")
    };

    output(
        json,
        ContextOutput {
            since_days: since.0,
            max_tokens,
            estimated_tokens: out.estimated_tokens,
            days: out
                .days
                .into_iter()
                .map(|(date, content)| ContextDayOutput {
                    date: date.format("%Y-%m-%d").to_string(),
                    content,
                })
                .collect(),
        },
        plain,
    )
}

/// MCP サーバを起動する。非同期が要るのはこのコマンドだけなので、
/// `main()` 全体を async にはせず、ここだけでランタイムを組み立てて
/// block_on する。
fn run_mcp(cfg: &Config) -> anyhow::Result<()> {
    let server = PenMcp::new(cfg.notes_dir.clone(), cfg.merge_window_minutes);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        anyhow::Ok(())
    })
}

enum Screen {
    Calendar,
    SearchInput,
    SearchResults,
}

/// raw mode/代替スクリーンのままエディタを起動すると表示が壊れるので、
/// 必ず端末を戻してから spawn し、終わったら入り直す。カレンダーからの
/// Enter と検索結果からの Enter の両方で使う。
fn open_and_resume(
    terminal: &mut ratatui::DefaultTerminal,
    cfg: &Config,
    date: NaiveDate,
) -> anyhow::Result<()> {
    ratatui::try_restore()?;
    let opened = notes::open_in_editor(&cfg.notes_dir, date, cfg.merge_window_minutes, &cfg.editor);
    *terminal = ratatui::try_init()?;
    opened?;
    Ok(())
}

fn run_cal(cfg: &Config) -> anyhow::Result<()> {
    let keymap = action::KeyMap::from_config(&cfg.keys)?;
    let mut terminal = ratatui::try_init()?;
    let mut calendar_state = CalendarState::new(Local::now().date_naive());
    let mut search_state = SearchState::new();
    let mut screen = Screen::Calendar;

    let result = (|| -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| match screen {
                Screen::Calendar => calendar::draw(frame, &calendar_state, &cfg.notes_dir),
                Screen::SearchInput => search::draw_input(frame, &search_state),
                Screen::SearchResults => search::draw_results(frame, &search_state),
            })?;

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let mode = match screen {
                Screen::Calendar => Mode::Calendar,
                Screen::SearchInput => Mode::SearchInput,
                Screen::SearchResults => Mode::SearchResults,
            };
            let Some(action) = action::resolve(&keymap, key, mode) else {
                continue;
            };

            match screen {
                Screen::Calendar => match calendar_state.apply(action) {
                    Some(CalendarRequest::Open(date)) => {
                        open_and_resume(&mut terminal, cfg, date)?;
                    }
                    Some(CalendarRequest::Search) => {
                        search_state = SearchState::new();
                        screen = Screen::SearchInput;
                    }
                    Some(CalendarRequest::Quit) => return Ok(()),
                    None => {}
                },
                Screen::SearchInput => match action {
                    Action::InputChar(c) => search_state.query.push(c),
                    Action::Backspace => {
                        search_state.query.pop();
                    }
                    Action::Confirm => match notes::search(&cfg.notes_dir, &search_state.query) {
                        Ok(results) => {
                            search_state.results = results;
                            search_state.selected = 0;
                            search_state.error = None;
                            screen = Screen::SearchResults;
                        }
                        Err(err) => search_state.error = Some(err.to_string()),
                    },
                    Action::Cancel => screen = Screen::Calendar,
                    _ => {}
                },
                Screen::SearchResults => match action {
                    Action::NextResult => search_state.select_next(),
                    Action::PrevResult => search_state.select_prev(),
                    Action::Confirm => {
                        screen = Screen::Calendar;
                        if let Some(date) = search_state.selected_hit().map(|hit| hit.date) {
                            calendar_state.selected = date;
                            open_and_resume(&mut terminal, cfg, date)?;
                        }
                    }
                    Action::Cancel => screen = Screen::Calendar,
                    _ => {}
                },
            }
        }
    })();

    ratatui::try_restore()?;
    result
}
