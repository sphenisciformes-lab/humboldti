# Changelog

All notable user-facing changes to Humboldti Note are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Before 1.0, a minor version bump (0.1 → 0.2) may include breaking changes;
they are always listed under **Changed** or **Removed**.

## Unreleased

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
