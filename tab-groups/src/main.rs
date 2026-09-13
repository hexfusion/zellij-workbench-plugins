//! tab-groups renders workbench tabs in two levels.
//!
//! Group membership comes from plugin configuration: each config entry is one
//! group, its key the group name and its value a comma-separated list of member
//! tab names. A tab in a group collapses under a clickable group chip on the top
//! row; the active group's members expand as sub-tabs on the row below. Tabs in
//! no group render as standalone top-row entries. Left-clicking a chip or a
//! sub-tab switches to it. Zellij tabs stay flat underneath; this plugin only
//! changes how they are presented and navigated.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    /// Groups in configured order: (group name, member tab names).
    groups: Vec<(String, Vec<String>)>,
    /// Clickable regions from the last render: (row, start_col, end_col, tab position).
    hits: Vec<(usize, usize, usize, usize)>,
}

impl ZellijPlugin for State {
    fn load(&mut self, config: BTreeMap<String, String>) {
        self.groups = config
            .into_iter()
            .map(|(name, members)| {
                let members = members
                    .split(',')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(str::to_owned)
                    .collect();
                (name, members)
            })
            .collect();
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
            },
            Event::Mouse(Mouse::LeftClick(line, col)) => {
                let row = usize::try_from(line).unwrap_or(0);
                if let Some(pos) = self.hit(row, col) {
                    // switch_tab_to is 1-based; TabInfo.position is 0-based.
                    switch_tab_to(u32::try_from(pos).unwrap_or(0).saturating_add(1));
                }
                false
            },
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let mut hits: Vec<(usize, usize, usize, usize)> = Vec::new();

        let active_group = self
            .tabs
            .iter()
            .find(|t| t.active)
            .and_then(|t| self.group_of(&t.name))
            .map(str::to_owned);

        // Top row: standalone tabs and one chip per group, in tab order.
        let mut top = String::new();
        let mut col = 0usize;
        let mut seen: Vec<String> = Vec::new();
        for t in &self.tabs {
            match self.group_of(&t.name) {
                None => {
                    let seg = format!(" {} ", t.name);
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
                    if let Some(target) = self.group_target(g) {
                        push_seg(&mut top, &mut hits, &mut col, 0, &seg, on, target);
                    }
                },
            }
        }
        top.push_str(&ansi(250, 236, &" ".repeat(cols.saturating_sub(col))));

        // Bottom row: the active group's members, in configured order.
        let mut bottom = String::new();
        if rows >= 2 {
            let mut bcol = 0usize;
            if let Some(g) = active_group.as_deref() {
                for member in self.members(g) {
                    if let Some(t) = self.tabs.iter().find(|t| &t.name == member) {
                        let seg = format!(" {} ", t.name);
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

impl State {
    /// The group whose members include `name`, if any.
    fn group_of(&self, name: &str) -> Option<&str> {
        self.groups
            .iter()
            .find(|(_, members)| members.iter().any(|m| m == name))
            .map(|(group, _)| group.as_str())
    }

    /// The configured members of `group`.
    fn members(&self, group: &str) -> &[String] {
        self.groups
            .iter()
            .find(|(name, _)| name == group)
            .map_or(&[], |(_, members)| members.as_slice())
    }

    /// Tab position a group chip switches to: the group's active member if any,
    /// otherwise its first member that exists as a live tab.
    fn group_target(&self, group: &str) -> Option<usize> {
        let live = || {
            self.members(group)
                .iter()
                .filter_map(|m| self.tabs.iter().find(|t| &t.name == m))
        };
        live().find(|t| t.active).or_else(|| live().next()).map(|t| t.position)
    }

    /// Resolve a click at (row, col) to a tab position.
    fn hit(&self, row: usize, col: usize) -> Option<usize> {
        self.hits
            .iter()
            .find(|(r, start, end, _)| *r == row && col >= *start && col < *end)
            .map(|&(_, _, _, pos)| pos)
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
