# Humboldti Note

A minimal daily markdown note-taking tool for your terminal.

**Status: functional for daily use.**

Japanese README: [README.ja.md](README.ja.md)

## Features

- **Frictionless capture.** `pen <text>` writes to today's file and exits — no editor, no prompt.
- **Checklists without the syntax.** `pen -t <text>` writes it as `- [ ] <text>`.
- **Nothing gets lost.** Unfinished `- [ ]` items carry over into today's file automatically, labeled with the day they came from.
- **A calendar you can actually read.** `pen cal` shows a month grid with a density heatmap of how much you wrote each day, adapted to your terminal's own light/dark colors.
- **Full-text search**, from the command line or from inside the calendar.
- **Built for Japanese.** Correct East Asian display-width handling everywhere text is laid out, not just wherever it was easiest to test.
- **Reconfigurable keys.** Every keybinding in `pen cal` can be rebound in `config.toml`.
- **Agent-ready.** `--json` on every command, a token-budgeted `context` dump for pasting into a chat, and a full MCP server for AI agents that can hold a connection open.
- **Plain files, forever.** One markdown file per day under `~/notes` — no database, no proprietary format. `grep`, `git`, and cloud sync already work on it.

## What it does

Humboldti Note has two bare forms — run with no subcommand keyword — and five
subcommands.

**Bare invocation**

| Run | Does |
|---|---|
| `pen <text>` | Append text to today's file |
| `pen -t <text>` | Same, formatted as a checklist item (`- [ ] <text>`) |
| `pen -` | Read the text to append from stdin instead |
| `pen` | Open today's file in `$EDITOR` |

`<text>` is literally whatever you type, e.g. `pen bought milk`. Consecutive
appends within `merge_window_minutes` (default 30) share one time heading
instead of starting a new one each time, and writes take an exclusive file
lock, so two terminals appending at once can't corrupt the file.

The first append or `open`/`pen cal` visit of a new day carries over any
unfinished `- [ ]` items from your most recent previous note, right above a
`<!-- carried over from YYYY-MM-DD -->` comment so you know where they came
from — see [Notes format](#notes-format) for what that looks like on disk.

**Subcommands**

| Run | Does |
|---|---|
| `pen open [<date>]` | Open a note in `$EDITOR`. `<date>` is `YYYY-MM-DD`; omit it for today |
| `pen cal` | Browse notes in a calendar view |
| `pen search <query>` | Full-text search (case-insensitive regular expression) |
| `pen context [--since 7d]` | Print recent notes in a form meant for feeding to an LLM |
| `pen config <path\|init\|check>` | Inspect or create the config file |
| `pen mcp` | Expose your notes to AI agents over MCP |

Every command supports `--json` for scripting and agent use, except `cal` (an
interactive TUI) and `mcp` (an MCP server, not a print-and-exit command). See
[Calendar and search](#calendar-and-search), [Configuration](#configuration),
[Scripting and feeding notes to an LLM](#scripting-and-feeding-notes-to-an-llm),
and [Using with AI agents](#using-with-ai-agents-mcp) below for each in more
detail.

## Calendar and search

`pen cal` opens a full-screen calendar:

- A Sunday-first month grid.
- Each day's background reflects how much you wrote that day (none / some /
  a lot), using your terminal's own ANSI colors — so it looks right in both
  light and dark themes instead of assuming a dark background.
- A preview pane shows the selected day's note (hidden automatically on
  narrower terminals).
- Default keys: `hjkl`/arrows move by day/week, `[`/`]` jump by month,
  `{`/`}` jump by year, `Enter` opens the selected day in `$EDITOR`, `/`
  starts a search, `q`/`Esc` quits. Every one of these is rebindable — see
  [Configuration](#configuration).

Press `/` from the calendar, or run `pen search <query>` directly, to
search: type a query and press `Enter`, then `j`/`k` to move between
results, `Enter` to open the selected day, `q`/`Esc` to go back. A plain
word like `meeting` matches literally too, since it's just a regular
expression.

## Notes format

Notes are plain `.md` files, one per day, under `~/notes`:

```
~/notes/
  2026/09/2026-09-05.md
  attachments/            # images, referenced by relative path
```

No database, no proprietary format, no lock-in — `grep`, `cat`, and `find`
already work. Point `git` or your favourite cloud sync at `~/notes` and it
just works. A carried-over checklist item looks like this in the file:

```markdown
<!-- carried over from 2026-09-02 -->
- [ ] something you didn't finish

## 21:07
whatever you actually wrote today
```

## Why

Terminal note tools exist, but a lightweight, simple one that combines daily
notes, a calendar, and frictionless capture is hard to find. One that also
gets Japanese text width and IME behaviour right is rarer still. Humboldti
Note is built for both.

## Install

**Homebrew** (macOS and Linux)

```sh
brew install sphenisciformes-lab/humboldti/humboldti-note
```

**Install script** (macOS and Linux, no Rust toolchain required)

```sh
curl -fsSL https://github.com/sphenisciformes-lab/humboldti/releases/latest/download/humboldti-note-installer.sh | sh
```

**cargo** (any platform with a Rust toolchain)

```sh
cargo install humboldti-note
```

**From source**

```sh
git clone https://github.com/sphenisciformes-lab/humboldti.git
cd humboldti
make install   # or: cargo install --path .
```

All four put a `pen` binary on your PATH.

## Configuration

Entirely optional — Humboldti Note works with zero setup. If you want to
change where notes live, or rebind `pen cal`'s keys, run:

```sh
pen config init
```

This writes a fully-commented config file to `~/.config/pen/config.toml`
(or `$XDG_CONFIG_HOME/pen/config.toml` if that's set), with every value
already at its built-in default — edit only what you want to change, and
delete the rest; missing keys just fall back. `pen config path` prints the
resolved path without creating anything, and `pen config check` prints
what's actually in effect once CLI flags, environment variables, and the
file are all applied.

Precedence: `--dir` flag > `PEN_*` environment variables > config file >
built-in defaults.

| Key | Controls |
|---|---|
| `notes_dir` | Where daily notes are written |
| `merge_window_minutes` | Minutes before a new append starts a new time heading |
| `editor` | Command used to open a note; falls back to `$EDITOR`, then `vi` |
| `[keys.calendar]` | Keybindings for the calendar screen |
| `[keys.search_input]` | Keybindings while typing a search query |
| `[keys.search_results]` | Keybindings while browsing search results |

If `editor` is a GUI editor, include its "wait for the window to close" flag
(`code --wait`, `subl --wait`) — otherwise `pen` thinks you're done editing as
soon as the launcher command returns, not when you actually close the window.

See the comments in a `config init`-generated file for the key-spec syntax
(`"h"`, `"ctrl-a"`, `"enter"`, ...). An unrecognized key anywhere in the file
just warns; an unparseable key spec, or two actions fighting over the same
key in one table, fails to start `pen cal`.

## Scripting and feeding notes to an LLM

Notes are plain markdown, so `grep`/`cat`/`find` already work without any of
this. Three more integration points build on that, each for a different kind
of consumer — they're not redundant, they just assume different capabilities
on the other end:

| | For | Output | Lifetime |
|---|---|---|---|
| `--json` | scripts and cron jobs | structured JSON | runs once, exits |
| `pen context` | pasting into a chat | a token-budgeted text dump | runs once, exits |
| `pen mcp` | an always-on AI agent | tools called on demand | stays running |

If your integration can only shell out and parse text, reach for `--json`. If
it can only take a block of text (a chat window, a script that calls an LLM
API once), reach for `context`. If it can hold an MCP connection open and
call tools on demand, use `mcp` — see [Using with AI agents](#using-with-ai-agents-mcp)
below.

### `--json`

It works whether you put `--json` before or after the subcommand name:

```sh
$ pen --json search 会議
{"hits":[{"path":"/Users/you/notes/2026/09/2026-09-01.md","date":"2026-09-01","line_number":2,"line":"チームMTGで来週のリリースについて会議した"}]}

$ pen --json config check
{"notes_dir":"/Users/you/notes","merge_window_minutes":30,"editor":""}
```

### `pen context`

Walks backward from today, day by day, until the token budget
(`--max-tokens`, default 4000, a rough character-based estimate) would be
exceeded. Never splits a day's file partway through, and always returns at
least the most recent day even if that alone is over budget.

```sh
$ pen context --since 7d
# 2026-09-01
## 10:00
チームMTGで来週のリリースについて会議した

<!-- estimated tokens: 16 / budget: 4000 -->

$ pen context --since 7d | pbcopy    # paste into a chat
$ pen --json context --since 2w > fortnight.json    # feed to a script
```

## Using with AI agents (MCP)

`pen mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io)
server over stdio, exposing three tools that call straight into the same code
`pen` uses on the command line:

- `search_notes(query)` — case-insensitive regex search across all notes
- `read_note(date)` — read one day's note (`YYYY-MM-DD`)
- `append_note(text)` — append to today's note, same as `pen <text>`

**Data flow.** Humboldti Note itself speaks stdio only — it never sends your
notes anywhere on its own. But whatever client you connect it to might: if
you point a cloud-based AI client at it, the content those tools read leaves
your machine and goes to that service. Check where your MCP client sends data
before connecting it to your notes.

Something needs to actually launch `pen mcp` as a subprocess — either you
configure that yourself, or (with Claude Desktop below) the client does it for
you.

### Claude Code

```sh
claude mcp add pen -- pen mcp
```

### Any other MCP client (manual config)

Point the client's MCP config at the `pen` binary directly:

```json
{
  "mcpServers": {
    "pen": {
      "command": "pen",
      "args": ["mcp"]
    }
  }
}
```

### Claude Desktop (one-click extension)

Claude Desktop installs local MCP servers as `.mcpb` extensions rather than
through a hand-edited config file. Build one yourself — this needs Node.js
only for the `mcpb` packaging CLI, not for Humboldti Note itself:

```sh
cargo build --release
mkdir -p mcpb/server
cp target/release/pen mcpb/server/pen
npx --yes @anthropic-ai/mcpb pack mcpb humboldti-note.mcpb
```

Then in Claude Desktop: Settings → Extensions → Advanced settings → Extension
developer → Install extension..., and select the `humboldti-note.mcpb` file
you just built. `mcpb/manifest.json` is what tells Claude Desktop to run
`pen mcp` on your behalf — you never type the command yourself.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
