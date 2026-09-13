use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    instance: String,
    focused_tab: String,
    text: String,
    class: String,
    /// Agent name the focused tab's session is running on.
    agent: String,
    /// Runtime behind that agent, which selects the badge colour.
    runtime: String,
    /// Target of a queued migrate/handoff, empty when there is none.
    pending_agent: String,
    /// True while a `tab info` call is in flight.
    pending: bool,
}

const REFRESH_SECS: u64 = 5;

impl ZellijPlugin for State {
    fn load(&mut self, _config: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::RunCommands,
        ]);
        subscribe(&[
            EventType::TabUpdate,
            EventType::Timer,
            EventType::RunCommandResult,
            EventType::Visible,
        ]);

        self.instance = std::env::var("ZELLIJ_SESSION_NAME").unwrap_or_default();
        self.class = "ok".to_string();
        self.text = format!(" {} ", self.instance);

        set_timeout(REFRESH_SECS as f64);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => {
                let new_focus = tabs
                    .iter()
                    .find(|t| t.active)
                    .map(|t| t.name.clone())
                    .unwrap_or_default();
                if new_focus != self.focused_tab {
                    self.focused_tab = new_focus;
                    // Reset pending so a stale in-flight call doesn't block us
                    self.pending = false;
                    self.request_refresh();
                    return true;
                }
            }
            Event::Visible(visible) => {
                if visible {
                    self.pending = false;
                    self.request_refresh();
                    return true;
                }
            }
            Event::Timer(_) => {
                self.request_refresh();
                set_timeout(REFRESH_SECS as f64);
                return false;
            }
            Event::RunCommandResult(_exit, stdout, _stderr, _ctx) => {
                self.pending = false;
                if let Ok(s) = std::str::from_utf8(&stdout) {
                    // Parse the JSON output minimally.
                    self.text = extract_field(s, "text").unwrap_or_default();
                    self.class = extract_field(s, "class").unwrap_or_else(|| "ok".to_string());
                    self.agent = extract_field(s, "agent").unwrap_or_default();
                    self.runtime = extract_field(s, "runtime").unwrap_or_default();
                    self.pending_agent = extract_field(s, "pending").unwrap_or_default();
                    if self.text.is_empty() {
                        self.text = format!(" {} ", self.focused_tab);
                    } else {
                        self.text = format!(" {} ", self.text);
                    }
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let (color_fg, color_bg) = match self.class.as_str() {
            "critical" => (255, 88),
            "warning" => (0, 214),
            _ => (255, 24),
        };

        let mut out = String::new();
        let mut used = 0usize;

        // Which account is answering matters more than any other number on this
        // bar, so it gets its own segment coloured by runtime rather than being
        // folded into the text.
        if !self.agent.is_empty() {
            let (fg, bg) = runtime_colors(&self.runtime);
            let label = format!(" {} ", self.agent);
            used += label.chars().count();
            out.push_str(&ansi(fg, bg, &label));

            if !self.pending_agent.is_empty() {
                let queued = format!(" \u{25b8} {} ", self.pending_agent);
                used += queued.chars().count();
                out.push_str(&ansi(0, 220, &queued));
            }
        }

        used += self.text.chars().count();
        let pad = cols.saturating_sub(used);
        out.push_str(&ansi(
            color_fg,
            color_bg,
            &format!("{}{}", self.text, " ".repeat(pad)),
        ));

        print!("{}", out);
    }
}

impl State {
    fn request_refresh(&mut self) {
        if self.instance.is_empty() || self.focused_tab.is_empty() || self.pending {
            return;
        }
        self.pending = true;
        run_command(
            &["workbench", "tab", "info", &self.instance, &self.focused_tab],
            BTreeMap::new(),
        );
    }
}

fn ansi(fg: u8, bg: u8, body: &str) -> String {
    format!("\u{1b}[38;5;{};48;5;{}m{}\u{1b}[0m", fg, bg, body)
}

/// Colour the agent badge by runtime so a glance tells you which provider is
/// answering, without having to read the name.
fn runtime_colors(runtime: &str) -> (u8, u8) {
    match runtime {
        "claude-code" => (231, 130),
        "codex" => (231, 30),
        _ => (231, 240),
    }
}

// Extracts a string field value from JSON like {"text":"...", "class":"..."}.
// Tolerant — does not handle escaped quotes inside the value.
fn extract_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":\"", key);
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// Manual register_plugin expansion to ensure `main` is exported on wasm32-wasip1.
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
        let protobuf_event: ProtobufEvent =
            ProtobufEvent::decode(protobuf_bytes.as_slice()).unwrap();
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
