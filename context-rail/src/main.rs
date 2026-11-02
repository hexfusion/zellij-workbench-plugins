//! context-rail is a vertical side-pane plugin that shows the focused session's
//! context and its group's roster, so the user can see the agents on a topic
//! and what each is doing at a glance.
//!
//! It reads the workbench roster (`workbench roster list <instance> --json`)
//! on a timer and on tab focus changes, filters to the focused tab's group,
//! and renders one block per session: a state dot, the tab, its agent, and the
//! latest activity note.

use std::collections::BTreeMap;

use serde::Deserialize;
use zellij_tile::prelude::*;

const REFRESH_SECS: f64 = 5.0;

#[derive(Default, Deserialize, Clone)]
struct Entry {
    #[serde(default)]
    tab: String,
    #[serde(default)]
    group: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    hash6: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    role: String,
}

#[derive(Default)]
struct State {
    instance: String,
    focused: String,
    entries: Vec<Entry>,
    pending: bool,
}

impl State {
    fn refresh(&mut self) {
        if self.instance.is_empty() || self.pending {
            return;
        }
        self.pending = true;
        run_command(
            &["workbench", "roster", "list", &self.instance, "--json"],
            BTreeMap::new(),
        );
    }

    fn focused_group(&self) -> &str {
        self.entries
            .iter()
            .find(|e| e.tab == self.focused)
            .map_or("", |e| e.group.as_str())
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _config: BTreeMap<String, String>) {
        request_permission(&[PermissionType::ReadApplicationState, PermissionType::RunCommands]);
        subscribe(&[EventType::TabUpdate, EventType::Timer, EventType::RunCommandResult]);
        self.instance = std::env::var("ZELLIJ_SESSION_NAME").unwrap_or_default();
        set_timeout(REFRESH_SECS);
        self.refresh();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => {
                let focused = tabs
                    .iter()
                    .find(|t| t.active)
                    .map(|t| t.name.clone())
                    .unwrap_or_default();
                if focused != self.focused {
                    self.focused = focused;
                    self.refresh();
                    return true;
                }
            },
            Event::Timer(_) => {
                self.refresh();
                set_timeout(REFRESH_SECS);
            },
            Event::RunCommandResult(_exit, stdout, _stderr, _ctx) => {
                self.pending = false;
                if let Ok(list) = serde_json::from_slice::<Vec<Entry>>(&stdout) {
                    self.entries = list;
                    return true;
                }
            },
            _ => {},
        }
        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let width = cols.max(1);
        let mut lines: Vec<String> = Vec::new();

        let group = self.focused_group().to_owned();
        let focused_label = if group.is_empty() {
            self.focused.clone()
        } else {
            format!("{group}/{}", self.focused)
        };
        let focused_entry = self.entries.iter().find(|e| e.tab == self.focused);
        let hash = focused_entry.map_or("", |e| e.hash6.as_str());

        lines.push(hl(16, 68, &pad(" context", width)));
        lines.push(fg(252, &trunc(&format!(" {focused_label}  #{hash}"), width)));
        if let Some(base) = focused_entry
            .map(|e| e.cwd.as_str())
            .filter(|c| !c.is_empty())
            .map(|c| c.rsplit('/').next().unwrap_or(c))
        {
            lines.push(fg(245, &trunc(&format!("   {base}"), width)));
        }
        lines.push(String::new());

        let members: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| if group.is_empty() { e.group.is_empty() } else { e.group == group })
            .collect();

        if members.is_empty() {
            lines.push(fg(244, &trunc(" no sessions yet", width)));
        } else {
            let title = if group.is_empty() {
                " sessions".to_owned()
            } else {
                format!(" {group} ({})", members.len())
            };
            lines.push(hl(250, 236, &pad(&title, width)));
            for e in members {
                let (dot, dot_fg) = match e.status.as_str() {
                    "live" => ('\u{25cf}', 46),
                    "stopped" => ('\u{25cb}', 240),
                    _ => ('\u{25d0}', 214),
                };
                let head = format!("{}  {}", e.tab, e.agent);
                if e.tab == self.focused {
                    lines.push(hl(16, 150, &pad(&trunc(&format!(" {dot} {head}"), width), width)));
                } else {
                    let rest = trunc(&head, width.saturating_sub(3));
                    lines.push(format!("{} {}", fg(dot_fg, &format!(" {dot}")), fg(252, &rest)));
                }
                let note = if e.note.is_empty() { e.role.as_str() } else { e.note.as_str() };
                if !note.is_empty() {
                    lines.push(fg(245, &trunc(&format!("    {note}"), width)));
                }
            }
        }

        let body = lines.into_iter().take(rows.max(1)).collect::<Vec<_>>().join("\n\r");
        print!("{body}");
    }
}

/// Truncate `s` to at most `width` characters.
fn trunc(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

/// Truncate to `width`, then pad with spaces to exactly `width`.
fn pad(s: &str, width: usize) -> String {
    let mut t: String = s.chars().take(width).collect();
    let len = t.chars().count();
    if len < width {
        t.push_str(&" ".repeat(width - len));
    }
    t
}

/// Foreground-only 256-color span.
fn fg(color: u8, body: &str) -> String {
    format!("\u{1b}[38;5;{color}m{body}\u{1b}[0m")
}

/// Foreground plus background 256-color span.
fn hl(fgc: u8, bgc: u8, body: &str) -> String {
    format!("\u{1b}[38;5;{fgc};48;5;{bgc}m{body}\u{1b}[0m")
}

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
