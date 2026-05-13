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

```
Event → Msg → update(&mut Model, Msg) → Option<Cmd>
                                              ↓
                                        Runtime::handle_cmd(Cmd) → Option<Msg>
                                              ↓ (loops back)
render(&Model, &mut WidgetState) → Frame
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
