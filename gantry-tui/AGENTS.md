# Architecture

The TUI follows The Elm Architecture (TEA): a single `Model` holds all app state, a
single `update` function transforms it in response to `Msg` values, and a single `render`
function projects it onto the screen each frame. There is no mutable state outside of
`Model`, and no rendering logic inside `update`.

## Module layout

Each UI feature lives in its own directory with two files:

```
chat/
  state.rs   – data only; no ratatui imports
  widget.rs  – rendering only; borrows from state
```

- **`state.rs`** defines the feature's state struct (`ChatState`, `InputState`, …).
  It contains fields, constructors, and methods that mutate or query the state.
  It must not import ratatui.
- **`widget.rs`** defines the ratatui widget struct (`ChatWidget`, `InputWidget`, …).
  It borrows from state at render time and is discarded after each frame.
  It must not own or mutate state.

The feature directory's `mod.rs` re-exports both the state type and the widget type so
callers can write `use crate::chat::{ChatState, ChatWidget}`.

## Naming conventions

| Concept | Suffix | Example |
|---|---|---|
| Feature state (nested in `Model`) | `State` | `ChatState`, `SessionsState` |
| Ratatui widget (ephemeral, borrows state) | `Widget` | `ChatWidget`, `SessionsWidget` |
| Ratatui widget's own render state | `WidgetState` | `ChatWidgetState` |
| Top-level app model | — | `Model` |
| Top-level render state | `WidgetState` | `WidgetState` |

`InputOverlay` variant names are short and match the feature name, not the type name:
`Usage`, `Sessions`, `Tree`, `CommandPicker`, `ModelPicker`, `AttachmentPicker`, `Providers`.

## Message flow

```mermaid
flowchart TD
    A[External source\nkeyboard / mouse / async result] -->|Msg| B[update\n&mut Model, Msg]
    B -->|mutates Model| C{Option&lt;Cmd&gt;}
    C -->|None| D[render\n&Model, &mut WidgetState]
    C -->|Some\(Cmd\)| E[Runtime::handle_cmd\nI/O, async tasks]
    E -->|Option&lt;Msg&gt;| B
    D --> F[Frame]
```

`Msg` — pure model-update messages. Every input event, stream event, or async result
that needs to change `Model` arrives as a `Msg`. `update()` is a pure function: it
mutates `Model` and returns an optional `Cmd`.

`Cmd` — side-effect commands returned by `update()`. `Runtime` executes them (I/O,
async tasks, app mutations) and may produce a follow-up `Msg` that re-enters the loop.

`Runtime` owns an internal `Event` channel that carries either a `Msg` or a `Cmd`.
External sources (keyboard, mouse, app events) send `Msg` values. Commands that need
to be re-dispatched (e.g. `Quit`, `NewSession` from within `run_command`) are sent as
`Cmd` values directly into the channel.

## Widget rendering patterns

**Use `Layout` to decompose space, not manual coordinate arithmetic.**
Split a widget's inner area into named sub-rects with `Layout::areas()` and render each
child into its own `Rect`. Do not compute positions by offsetting `inner.x`/`inner.y`
manually — that couples layout to magic constants and makes structural intent invisible.

```rust
// Good
let [prompt_area, _, list_area, counter_area] = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(list_height),
        Constraint::Length(1),
    ])
    .areas(inner);

some_widget.render(list_area, buf);

// Bad
let list = Rect::new(inner.x, inner.y + LIST_Y_OFFSET, inner.width, …);
some_widget.render(list, buf);
```

**Render into a `Rect`, never write directly to `buf`.**
Prefer `widget.render(area, buf)` over `buf.set_string(x, y, …)`. Even plain text should
be wrapped in a `Line` or `Span` and rendered into its layout area so all content goes
through the same path.
