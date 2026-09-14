# Major GUI Pass 6 — Visual Reconstruction

This pass responds directly to the captured macOS reference screenshots and
recording. Pass 5 established the right domains but still looked like a
prototype: too many large toolbar buttons, weak pane hierarchy, no server-mode
workspace, no file icons/date column, and an underdeveloped Inspector.

## Visual hierarchy

- compact mode strip instead of an oversized command toolbar
- right workspace modes: Browser / Servers / Quick Connect
- compact icon command bar
- contextual search for the active pane
- tighter pane spacing and splitters
- pane location band + navigation row
- dense native list presentation with file/folder icons
- browser columns become Name / Size / Date

## Metadata

`FileEntry` gains optional modification time as a Unix timestamp.

- local backend populates it from native filesystem metadata
- SFTP backend populates it from `FileStat::mtime`
- Linux browser renders a Date column
- Inspector renders Modified metadata
- `cpk` exposes modification time in its list/stat boundary
- AppKit parser/table/Inspector consume the same field

## Inspector

The Inspector now has:

- large selection icon
- selected item name
- kind
- size
- modification time
- backend/location
- full path
- permission matrix foundation

Permission editing remains deliberately disabled until backend metadata-write
support exists.

## Servers / Quick Connect

The right side can now switch between:

- normal browser
- saved Servers view sourced from shared Rust connection profiles
- Quick Connect form

Opening a saved server or Quick Connect target connects the right pane and
returns it to browser mode.

## Toolbar cleanup

Connect/Local and large text actions are no longer the primary visual model.
The compact bar contains refresh, new folder, info, rename, delete, hidden,
activity, sort and reverse controls. Full commands remain available from menus
and context menus.

## macOS source lane

- CPK entry parser gains modification timestamps
- table becomes Name / Size / Date
- Inspector gains Modified metadata

The AppKit source lane still requires a macOS compile pass before it is called
green.
