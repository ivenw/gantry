# TODOs

## Features

- Message queuing
- Permissions
- CLI mode
- RPC mode
- LSP support
- Session usage
- usage vie
- stats view (compute stats based on session data over a given time span)
  last 7 days, last 30 days, last 365 days all time
  depends on usage implementation
- unify context, usage and stats view into one view that with a tab bar.
  navigate with either tab or left-right arrows
- encryption at rest of credentials
- use notify crate (with debounce mini) to watch file system for changes to agent.md and skill files
this would also mean we don't need the reload command
- reasoning
- compaction (protect loaded skills)
- edit mode (give access to tools that can make a change, probably including bash)
- enrich model list data with https://models.dev/api.json
- resume chat after interrupt (will require some special consideration how tool call interrupts
are handled)

### Commands
- reload to refresh the system_prompt with agent files and skills

### Tools
- Distribute them using ToolServer
- Web search tool
- Web fetch tool
- Skills

### System prompt
- current date
- cwd
- tool use guidance

## UI
- show feedback for input token expansion

### Chat
- Visualize tool usage better
- Maybe show error messages here instead of statusline?
- when interrupting, there should be a possibility to delete the incomplete response
- syntax highlighting support
- render diffhunks using `similar` crate
- only render results for read, edit, write. don't eagerly display the call

### Input
- paste
- copy (visual mode)
- user bash commands (start with ! in the input)
- + shortcut for adding files dirs to the context (read file, list dir)
- @ for mentioning other users once we have multiplayer implemented
- / for listing skills
- show mode status by coloring the input field to indicate "active" state when in insert

### Session browse/resume
- don't show id, show first user message
- dual pane view where the righ side shows the tree view of the currently selected session
- or dual pane with chat preview based on current leaf
- maybe tripple pane once we have a pulse implemented

### Statusline
- input/output token count per session
- context window token count / %
- error message shouldn't cover other things

### Provider config view
- confirm y/n before deletion
- enable editing of existing aliases
- add option to disable provider without deleting it

### Model selection
- implement fuzzy search over model name
- group models by provider into sections
- show a list of recently used models at the top
- show more metadata information about the models. context length etc.

### Context
- move context usage to `context` command
- Full path on AGENT files
- Show skill usage
- Full path on skills
- utilize horizontal space better. hard to read right now

### Usage
- stats on active session
- input, output, cache r/w tokens by model
- total and by model
- models ranked by total token usage

### Stats
- total sessions, sessions / day
- total tokens, tokens / day
- input, output, cache r/w tokens total, per day
- avg/median tokens / session
- token stats per model too
- model usage ranking

## Refactors
- use the atomic-write-file crate instead of our hand rolled solution (for configs and the edit tool)
- Rename `SessionHistory` to `SessionStore` (?)
- We may not need toml_edit
- Use layout pattern in all views (see command picker for example) basically just const for magic values

## Issues
- model has no idea what the current working dir is so it can't use tools effectively
tools need be cwd aware. we have to do this by building the tools with cwd so that it is baked in
but not part of the tools signature. should just be as simple as adding the cwd as a field on the
given tool struct
- copy paste from clipboard doesn't work. has to work in input but maybe should work where ever ther
is text input? we probably need a generic text input abstraction for that.
- serde_yaml is unmaintained. maybe use saphyr-yaml instead
- double check that we use the CWD and not project dir for tools search_paths discovery etc.
the project dir is only needed (eventually) for regetering a session in the central backend
- Tree tool takes a long time
- UI likes to freeze on occarsion. observe if this has to do with ollama blocking the computer
or another issue
- input doesn't clear on new session (should it though?)
- rigs tool calling makes it VERY cumbersome to return display relevant data like diffhunks that
should NOT be part of the tool output the agent sees. This might prompt us to implement our own
provider and agent implementation. Maybe a shorter path to victory is just implementing the tool calling
ourselves and only use rig for its providers at the moment.
- Since diffhunks arrive through a channel and just append to the oldest completed edit call
without hunks (at least matched by path), concurrent writes to the same file may lead to display
bugs caused by race conditions (the wrong hunk attached to the wront edit call). probably not an
issue in practice because the agent should anyway issue multiple edits to the same file to the same
edit call but still a weakspot in the current implementation.

## Bugs
- partial model responses (interupted) are not being persisted to the message history
- shift+enter doesn't work in tmux to insert a \n (works in other tuis)

## Ideas
- live agents.md discovery. add them automatically if one exists in a file either added with + or
read with the read tool? read tool probably will blow the context up. self discovery might be sufficient
- "ask user question" tool
- Voice dictation
