# zellij-workbench-plugins

Zellij plugins for [workbench](https://github.com/hexfusion/workbench), an AI-native workspace manager for zellij. Each plugin renders a slice of workbench state in a zellij pane.

## Plugins

### tab-groups

A tab-bar plugin that renders tabs in two levels. A tab named `group/name` collapses under a clickable `group` chip on the top row; the active group's members expand as sub-tabs on the row below. Left-clicking a chip or a sub-tab switches to it. Tabs without a `/` render as standalone entries. Zellij tabs stay flat underneath; the plugin only changes how they are presented and navigated.

### tab-stats

A status-line plugin that shows the focused tab's workbench session: the agent account answering it (colored by runtime), any queued migrate or handoff target, and spend. Reads `workbench tab info`.

## Planned

### context-rail

A narrow side-pane plugin for the session-centric layout (once the editor and files panes are removed). For the current session it shows its workstream, Jira, and worktree or branch, plus the sibling sessions in its group with their state and spend, so a session's identity and its group are always on screen. This is the observability surface that keeps multi-session work from crossing wires.

## Build

Requires the `wasm32-wasip1` target: `rustup target add wasm32-wasip1`.

    make           # build both plugins (release, wasm32-wasip1)
    make install   # build, then install to ~/.config/zellij/plugins/

Installed as `tab_groups.wasm` and `tab_stats.wasm`. Reference one from a zellij layout with `plugin location="file:~/.config/zellij/plugins/<name>.wasm"`.

## License

Apache-2.0.
