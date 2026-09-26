# cmdrerun

*English · [Русский](README.ru.md)*

A desktop app in Rust + [egui 0.36.1](https://github.com/emilk/egui) for storing and running
shell scripts: a command tree, git-like version history with diffs, Jenkins-style build
parameters, and a full run history.

## Running

```bash
cargo run --release
```

## Interface

**Main menu**

- **File** — import and export a command, quit.
- **Settings** — interface language (Russian / English).
- **Help** — "About" with a short overview of what the app does.

**Left — the tree.** Branches are groups, leaves are commands. The "+ Add" button opens
a menu: group, command, or import. Everything else is in the node's right-click context menu:
run, export, rename, delete.

A short click on a node opens it on the right. Press and drag to start moving it: the node's
name follows the cursor, and the drop target is highlighted. Near the edge of a row that's a
line ("place next to it"); over the middle of a group it's a frame around it ("place inside").

**Right — the selected item.** For a group, that's its path and stats about what's nested
inside. For a command, the panel splits in half vertically: top-left holds the properties
(name, comment, script), top-right the parameters, and the bottom has two history tabs — runs
and changes.

The selected node, expanded groups, and language are all remembered: the app reopens exactly
where you left it.

## Parameters

The types are modeled after Jenkins build parameters:

| Type | Input field |
| --- | --- |
| `String` | single-line input |
| `Text` | multi-line input |
| `Boolean` | checkbox (`true` / `false`) |
| `Choice` | dropdown of predefined options |
| `Password` | masked input |

Values replace `${NAME}` in the script and are also passed to the process as environment
variables. Other `${...}` are left untouched and go straight to the shell — `${HOME}` keeps
working as usual.

## Version history

Saving a command pushes the previous version in full (name, script, comment, parameters) into
the change history. The "Change history" tab shows a line-by-line diff between a version and
whatever replaced it:

- **green** — added;
- **red** — removed;
- **gray** — replaced.

The "Restore into editor" button loads the old version into the editor fields — applying it is
done with the regular "Save" button, so it lands in the history too.

## Concurrent runs

Different commands can run at the same time: a long build of one doesn't block starting
another — each run happens on its own thread. While a command is running somewhere in the
background, it's marked with an orange dot in the tree on the left. Starting an already-running
command again isn't possible — its "Run" button stays disabled until "Stop" appears.

## Running and progress

Unsaved edits are written automatically before a run, so the history always has the exact
version that was executed.

The command runs in `$SHELL -c` (`cmd /C` on Windows) on a background thread, with the home
directory as the working directory. `stdout` and `stderr` are read line by line and appear in
the UI as they happen; the "Stop" button kills the process.

The progress bar is built from the median duration of the command's last ten completed runs.
While a run is still going, the bar never reaches 100%; if a run takes longer than the median,
the label honestly says "longer than usual". There's no estimate for the very first run — it
just shows an indicator with no percentage.

`stdout` and `stderr` are shown as a single stream in the order the lines actually arrived —
otherwise it's unclear at which step the command failed. Lines from `stderr` are highlighted in
red, and in the history file they're marked with a `!` at the start of the line (ordinary lines
get two spaces instead). The merge is only as accurate as the streams themselves: a program
that buffers `stdout` will have it show up later than `stderr` — nothing to be done about that.

Next to the "Output" heading are two buttons: **⛶** opens the output in a separate window (the
full thing, and a live run keeps appending to it there too), **🗐** copies it to the clipboard
without the markers.

The full run is written to history: the script with parameters substituted, the parameter
values, the output, the exit code, and the start/end time. For each run you can expand a diff
against the current version of the command — showing how much the script has changed since
then. The history entry's context menu has "Run again" (parameter values are taken from that
run; passwords aren't written to history, so the command itself supplies them) and "Delete"
with confirmation.

**Passwords never touch disk.** In the saved script and in the parameter list, `Password`
values are replaced with `********`; the real value is only passed to the process itself.

## Import and export

- **Export** (`File → Export command…`, a button on the command panel, or the context menu):
  the `.json` extension saves the whole command — name, comment, script, and parameters; any
  other extension saves just the script.
- **Import** (`File → Import command…`): for a script, you can choose a mode.
  - **Copy** — the file's contents are copied into the store; the file itself isn't used
    afterward.
  - **Link** — the command stores the path to the original file: the script is read from it
    when the command is opened and written back to it when saved. Version history still works
    as usual. The link is shown next to the "Script" heading, along with "Reload" and "Unlink"
    buttons.
  - An export file (`.json`) is always imported whole, and always as a copy — there's nothing
    to link to.

## Storage

Everything is stored as plain JSON files. The default directory is the OS's standard data
directory (`~/Library/Application Support/cmdrerun` on macOS), and can be overridden with the
`CMDRERUN_HOME` environment variable:

```text
<dir>/settings.json                                     language, tree selection, expanded groups
<dir>/folders/<folder_id>.json
<dir>/commands/<command_id>.json
<dir>/history/<command_id>/runs/<timestamp>__<log_id>.json
<dir>/history/<command_id>/changes/<timestamp>__<change_id>.json
```

Files are written to a temporary file and then renamed into place, so an interrupted write
never leaves a corrupt JSON file behind. The current storage path is shown in the bottom-right
corner of the window.

## Data structures

```rust
enum ParamType { String, Text, Boolean, Choice(Vec<String>), Password }
struct Parameter    { name, value, param_type }
struct Command      { id, name, script, comment, params, parent_id, order, script_path }
struct Folder       { id, name, parent_id, order }
struct ExecutionLog { id, command_id, script, output, exit_code, start_time, end_time, params }
struct ChangeLog    { id, command_id, old_script, old_params, old_name, old_comment, timestamp }
```

## Development

```bash
cargo test
cargo clippy --all-targets
```

Interface strings live in `src/i18n.rs`: two constants, `RU` and `EN`, with the same set of
fields; a new language is added as a third constant plus a new `Lang` variant.
