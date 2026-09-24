# Changelog

All notable user-facing changes to Humboldti Note are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Before 1.0, a minor version bump (0.1 → 0.2) may include breaking changes;
they are always listed under **Changed** or **Removed**.

## Unreleased

### Added

- `pen cal` marks today (bold and underlined), so you can find it after moving
  the selection to another day.

### Changed

- The MCP `search_notes` tool returns at most 100 matching lines (newest
  first) and says how many more it left out, so one broad query can't fill an
  agent's context. Use `pen search` for the full list.
- `ctrl-c` can no longer be bound in `[keys.*]`; doing so fails to start
  `pen cal` with an error. It is reserved for quitting (see below).

### Fixed

- `Ctrl-C` now quits `pen cal` from any screen. It used to be ignored in the
  calendar and typed a `c` into the search query.
- Other `Ctrl` and `Alt` key combinations are no longer typed into the search
  query as plain letters.
- Uppercase keys in `[keys.*]` (`"G"`, or `"shift-g"`) now work. They were
  accepted but never matched, because the terminal reports them with Shift held.
- Tab-indented lines (such as nested `- [ ]` items) keep their indentation in
  the `pen cal` preview pane and search results. Tabs used to be dropped.
- The key hints in `pen cal` (the calendar's bottom line and the search screen
  titles) now show your configured keys. They were fixed text, so they showed
  the default keys even after you rebound them. Hints longer than the terminal
  is wide now end in `…` rather than being cut off without warning.
- `pen context --since` with a huge value (such as `4000000000d`) no longer
  crashes. The range is capped at about 100 years.
- A search query longer than the input box no longer pushes the cursor out of
  view. The box shows the end of the query, which is the part you're typing.

## [0.2.0] - 2026-09-24

### Changed

- The `editor` setting and `$EDITOR` are now run through `sh -c`, the same
  way git runs `core.editor`. Quote paths that contain spaces
  (`"/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl" -w`).
  Previously the command was split on whitespace, so such values did not work.
  A missing editor now fails with exit status 127, and the error names the editor.
- `pen search` and search in `pen cal` no longer skip notes matched by
  `.gitignore` or `.ignore` files, so a notes directory inside a repository
  that ignores it is searchable. Hidden directories (`.git`, `.trash`, …) are
  still skipped.

### Fixed

- `pen open` no longer deletes today's note if it cannot be read (for example,
  when it contains text that is not valid UTF-8).
- `pen open` now holds the note's file lock while it adds and removes the time
  heading, so a `pen <text>` running at the same time is no longer lost.
- The search results list in `pen cal` scrolls to keep the selected result
  visible.

[0.2.0]: https://github.com/sphenisciformes-lab/humboldti/compare/v0.1.2...v0.2.0
