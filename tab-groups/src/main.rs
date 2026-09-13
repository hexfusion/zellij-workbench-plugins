//! tab-groups renders workbench tabs in two levels.
//!
//! A tab named `group/name` collapses under a clickable `group` chip on the
//! top row; the active group's members expand as sub-tabs on the row below.
//! Left-clicking a chip or a sub-tab switches to it. Tabs without a `/` render
//! as standalone top-row entries. Zellij tabs stay flat underneath; this plugin
//! only changes how they are presented and navigated.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

/// Separator splitting a group prefix from the sub-tab label: `grid-tp/fips`
/// is sub-tab `fips` of group `grid-tp`.
const GROUP_SEP: char = '/';

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    /// Clickable regions from the last render: (row, start_col, end_col, tab position).
    hits: Vec<(usize, usize, usize, usize)>,
}

/// Split a tab name into an optional group and its label.
fn split_group(name: &str) -> (Option<&str>, &str) {
    match name.split_once(GROUP_SEP) {
        Some((group, label)) if !group.is_empty() && !label.is_empty() => (Some(group), label),
        _ => (None, name),
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _config: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[EventType::TabUpdate, EventType::Mouse]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
                true
            }
            Event::Mouse(Mouse::LeftClick(line, col)) => {
                let row = usize::try_from(line).unwrap_or(0);
                if let Some(pos) = self.hit(row, col) {
                    // switch_tab_to is 1-based; TabInfo.position is 0-based.
                    switch_tab_to(u32::try_from(pos).unwrap_or(0).saturating_add(1));
                }
                false
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let mut hits: Vec<(usize, usize, usize, usize)> = Vec::new();

        let active_group = self
            .tabs
            .iter()
            .find(|t| t.active)
            .and_then(|t| split_group(&t.name).0.map(str::to_string));

        // Top row: standalone tabs and one chip per group.
        let mut top = String::new();
        let mut col = 0usize;
        let mut seen: Vec<String> = Vec::new();
        for t in &self.tabs {
            let (group, label) = split_group(&t.name);
            match group {
                None => {
                    let seg = format!(" {label} ");
                    push_seg(&mut top, &mut hits, &mut col, 0, &seg, t.active, t.position);
                },
                Some(g) => {
                    if seen.iter().any(|s| s == g) {
                        continue;
                    }
                    seen.push(g.to_owned());
                    let on = active_group.as_deref() == Some(g);
                    let marker = if on { " \u{25be}" } else { "" };
                    let seg = format!(" {g}{marker} ");
                    let target = self.group_target(g);
                    push_seg(&mut top, &mut hits, &mut col, 0, &seg, on, target);
                },
            }
        }
        top.push_str(&ansi(250, 236, &" ".repeat(cols.saturating_sub(col))));

        // Bottom row: sub-tabs of the active group.
        let mut bottom = String::new();
        if rows >= 2 {
            let mut bcol = 0usize;
            if let Some(g) = active_group.as_deref() {
                for t in &self.tabs {
                    let (group, label) = split_group(&t.name);
                    if group == Some(g) {
                        let seg = format!(" {label} ");
                        push_seg(&mut bottom, &mut hits, &mut bcol, 1, &seg, t.active, t.position);
                    }
                }
            }
            bottom.push_str(&ansi(250, 233, &" ".repeat(cols.saturating_sub(bcol))));
        }

        self.hits = hits;
        if rows >= 2 {
            print!("{top}\n\r{bottom}");
        } else {
            print!("{top}");
        }
    }
}

/// Append a colored segment to `out` and record its clickable region in `hits`.
fn push_seg(
    out: &mut String,
    hits: &mut Vec<(usize, usize, usize, usize)>,
    col: &mut usize,
    row: usize,
    seg: &str,
    active: bool,
    pos: usize,
) {
    let (fg, bg) = if active {
        (16, 150)
    } else if row == 0 {
        (250, 236)
    } else {
        (250, 233)
    };
    let width = seg.chars().count();
    hits.push((row, *col, *col + width, pos));
    *col += width;
    out.push_str(&ansi(fg, bg, seg));
}

impl State {
    /// Tab position a group chip switches to: the group's active member if any,
    /// otherwise its first member.
    fn group_target(&self, group: &str) -> usize {
        let members = || self.tabs.iter().filter(|t| split_group(&t.name).0 == Some(group));
        members()
            .find(|t| t.active)
            .or_else(|| members().next())
            .map_or(0, |t| t.position)
    }

    /// Resolve a click at (row, col) to a tab position.
    fn hit(&self, row: usize, col: usize) -> Option<usize> {
        self.hits
            .iter()
            .find(|(r, start, end, _)| *r == row && col >= *start && col < *end)
            .map(|&(_, _, _, pos)| pos)
    }
}

/// Wrap `body` in a 256-color foreground/background SGR pair.
fn ansi(fg: u8, bg: u8, body: &str) -> String {
    format!("\u{1b}[38;5;{fg};48;5;{bg}m{body}\u{1b}[0m")
}

// Manual register_plugin expansion, matching tab-stats, to ensure the wasm
// entry points are exported on wasm32-wasip1.
thread_local! {
    static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::default());
}

#[no_mangle]
pub fn main() {
    std::panic::set_hook(Box::new(|info| {
        report_panic(info);
    }));
}

#[no_mangle]
fn load() {
    STATE.with(|state| {
        use std::convert::TryFrom;
        use zellij_tile::shim::plugin_api::action::ProtobufPluginConfiguration;
        use zellij_tile::shim::prost::Message;
        let protobuf_bytes: Vec<u8> = zellij_tile::shim::object_from_stdin().unwrap();
        let protobuf_configuration: ProtobufPluginConfiguration =
            ProtobufPluginConfiguration::decode(protobuf_bytes.as_slice()).unwrap();
        let plugin_configuration: BTreeMap<String, String> =
            BTreeMap::try_from(&protobuf_configuration).unwrap();
        state.borrow_mut().load(plugin_configuration);
    });
}

#[no_mangle]
pub fn update() -> bool {
    use std::convert::TryInto;
    use zellij_tile::shim::plugin_api::event::ProtobufEvent;
    use zellij_tile::shim::prost::Message;
    STATE.with(|state| {
        let protobuf_bytes: Vec<u8> = zellij_tile::shim::object_from_stdin().unwrap();
        let protobuf_event: ProtobufEvent = ProtobufEvent::decode(protobuf_bytes.as_slice()).unwrap();
        let event: Event = protobuf_event.try_into().unwrap();
        state.borrow_mut().update(event)
    })
}

#[no_mangle]
pub fn pipe() -> bool {
    use std::convert::TryInto;
    use zellij_tile::shim::plugin_api::pipe_message::ProtobufPipeMessage;
    use zellij_tile::shim::prost::Message;
    STATE.with(|state| {
        let protobuf_bytes: Vec<u8> = zellij_tile::shim::object_from_stdin().unwrap();
        let protobuf_pipe_message: ProtobufPipeMessage =
            ProtobufPipeMessage::decode(protobuf_bytes.as_slice()).unwrap();
        let pipe_message = protobuf_pipe_message.try_into().unwrap();
        state.borrow_mut().pipe(pipe_message)
    })
}

#[no_mangle]
pub fn render(rows: i32, cols: i32) {
    STATE.with(|state| {
        state.borrow_mut().render(rows as usize, cols as usize);
    });
}

#[no_mangle]
pub fn plugin_version() {
    println!("{}", zellij_tile::prelude::VERSION);
}
