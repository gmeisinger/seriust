use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;

use crate::serial::{self, SerialCommand, SerialConfig, SerialEvent, SerialHandle};
use crate::{Args, ui};

pub const BAUD_RATES: &[u32] = &[
    300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];

#[derive(Debug, PartialEq, Eq)]
pub enum AppState {
    Capturing,
    Options,
    PortList,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum InputMode {
    Ascii,
    Hex,
}

impl InputMode {
    pub fn label(&self) -> &str {
        match self {
            InputMode::Ascii => "ASCII",
            InputMode::Hex => "HEX",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LineEnding {
    CrLf,
    Cr,
    Lf,
    None,
}

impl LineEnding {
    pub fn label(&self) -> &str {
        match self {
            LineEnding::CrLf => "CR+LF",
            LineEnding::Cr => "CR",
            LineEnding::Lf => "LF",
            LineEnding::None => "None",
        }
    }

    pub fn bytes(&self) -> &[u8] {
        match self {
            LineEnding::CrLf => b"\r\n",
            LineEnding::Cr => b"\r",
            LineEnding::Lf => b"\n",
            LineEnding::None => b"",
        }
    }
}

#[derive(Debug, Clone)]
pub enum MenuAction {
    SelectPort,
    Disconnect,
    CycleBaudRate,
    CycleDataBits,
    CycleParity,
    CycleStopBits,
    CycleFlowControl,
    CycleLocalEcho,
    CycleLineEnding,
    CycleInputMode,
    Exit,
}

#[derive(Debug, Clone)]
pub enum MenuItem {
    SectionHeader(String),
    Action {
        label: String,
        action: MenuAction,
    },
    Cycle {
        label: String,
        action: MenuAction,
        value: String,
    },
}

impl MenuItem {
    pub fn is_selectable(&self) -> bool {
        !matches!(self, MenuItem::SectionHeader(_))
    }
}

pub struct App {
    pub args: Args,
    pub exit: bool,
    pub serial_config: SerialConfig,
    pub app_state: AppState,
    pub input_buffer: String,
    pub output_buffer: String,
    pub available_ports: Vec<serialport::SerialPortInfo>,
    pub port_error: Option<String>,
    pub port_list_index: usize,
    pub connection_status: ConnectionStatus,
    pub local_echo: bool,
    pub input_mode: InputMode,
    pub line_ending: LineEnding,
    pub menu_cursor: usize,
    serial_handle: Option<SerialHandle>,
    last_port_scan: Instant,
    config_snapshot: Option<SerialConfig>,
}

impl App {
    pub fn new(args: Args) -> Self {
        Self {
            args,
            exit: false,
            serial_config: SerialConfig::default(),
            app_state: AppState::Capturing,
            input_buffer: String::new(),
            output_buffer: String::new(),
            available_ports: Vec::new(),
            port_error: None,
            port_list_index: 0,
            connection_status: ConnectionStatus::Disconnected,
            local_echo: true,
            input_mode: InputMode::Ascii,
            line_ending: LineEnding::CrLf,
            menu_cursor: 1,
            serial_handle: None,
            last_port_scan: Instant::now(),
            config_snapshot: None,
        }
    }

    pub fn build_menu_items(&self) -> Vec<MenuItem> {
        let port_label = self
            .serial_config
            .port_info
            .as_ref()
            .map(|p| p.port_name.clone())
            .unwrap_or_else(|| "—".to_string());

        let baud_val = self.serial_config.baud.to_string();
        let data_bits_val = match self.serial_config.data_bits {
            serialport::DataBits::Five => "5",
            serialport::DataBits::Six => "6",
            serialport::DataBits::Seven => "7",
            serialport::DataBits::Eight => "8",
        }
        .to_string();
        let parity_val = match self.serial_config.parity {
            serialport::Parity::None => "None",
            serialport::Parity::Odd => "Odd",
            serialport::Parity::Even => "Even",
        }
        .to_string();
        let stop_bits_val = match self.serial_config.stop_bits {
            serialport::StopBits::One => "1",
            serialport::StopBits::Two => "2",
        }
        .to_string();
        let flow_val = match self.serial_config.flow_control {
            serialport::FlowControl::None => "None",
            serialport::FlowControl::Hardware => "Hardware",
            serialport::FlowControl::Software => "Software",
        }
        .to_string();
        let echo_val = if self.local_echo { "ON" } else { "OFF" }.to_string();
        let line_ending_val = self.line_ending.label().to_string();
        let input_mode_val = self.input_mode.label().to_string();

        vec![
            MenuItem::SectionHeader("CONNECTION".to_string()),
            MenuItem::Action {
                label: format!("Select Port              {}", port_label),
                action: MenuAction::SelectPort,
            },
            MenuItem::Action {
                label: "Disconnect".to_string(),
                action: MenuAction::Disconnect,
            },
            MenuItem::SectionHeader("SERIAL CONFIG".to_string()),
            MenuItem::Cycle {
                label: "Baud Rate".to_string(),
                action: MenuAction::CycleBaudRate,
                value: baud_val,
            },
            MenuItem::Cycle {
                label: "Data Bits".to_string(),
                action: MenuAction::CycleDataBits,
                value: data_bits_val,
            },
            MenuItem::Cycle {
                label: "Parity".to_string(),
                action: MenuAction::CycleParity,
                value: parity_val,
            },
            MenuItem::Cycle {
                label: "Stop Bits".to_string(),
                action: MenuAction::CycleStopBits,
                value: stop_bits_val,
            },
            MenuItem::Cycle {
                label: "Flow Control".to_string(),
                action: MenuAction::CycleFlowControl,
                value: flow_val,
            },
            MenuItem::SectionHeader("DISPLAY".to_string()),
            MenuItem::Cycle {
                label: "Local Echo".to_string(),
                action: MenuAction::CycleLocalEcho,
                value: echo_val,
            },
            MenuItem::Cycle {
                label: "Line Ending".to_string(),
                action: MenuAction::CycleLineEnding,
                value: line_ending_val,
            },
            MenuItem::Cycle {
                label: "Input Mode".to_string(),
                action: MenuAction::CycleInputMode,
                value: input_mode_val,
            },
            MenuItem::Action {
                label: "Exit".to_string(),
                action: MenuAction::Exit,
            },
        ]
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // apply args
        self.serial_config.baud = self.args.baud;
        self.serial_config.data_bits = match self.args.data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            8 => serialport::DataBits::Eight,
            _ => serialport::DataBits::Eight,
        };
        self.serial_config.stop_bits = match self.args.stop_bits {
            2 => serialport::StopBits::Two,
            1 => serialport::StopBits::One,
            _ => serialport::StopBits::One,
        };
        self.serial_config.parity = match self.args.parity.as_str() {
            "odd" => serialport::Parity::Odd,
            "even" => serialport::Parity::Even,
            _ => serialport::Parity::None,
        };
        self.serial_config.flow_control = match self.args.flow_control.as_str() {
            "hardware" => serialport::FlowControl::Hardware,
            "software" => serialport::FlowControl::Software,
            _ => serialport::FlowControl::None,
        };

        if let Some(ref port_name) = self.args.port {
            self.serial_config.port_info = Some(serialport::SerialPortInfo {
                port_name: port_name.clone(),
                port_type: serialport::SerialPortType::Unknown,
            });
        }

        if self.serial_config.port_info.is_some() {
            self.try_connect();
        } else {
            self.app_state = AppState::Options;
        }

        self.scan_ports();

        while !self.exit {
            if self.last_port_scan.elapsed() >= Duration::from_secs(2) {
                self.scan_ports();
                self.last_port_scan = Instant::now();
            }

            self.drain_serial_events();
            terminal.draw(|frame| ui::draw(self, frame))?;
            self.handle_events()?;
        }

        if let Some(handle) = self.serial_handle.take() {
            handle.disconnect();
        }

        Ok(())
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)
                }
                _ => {}
            };
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        // Ctrl+A toggles menu from any state
        if key_event.code == KeyCode::Char('a')
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            if self.app_state == AppState::Capturing {
                self.open_menu();
            } else {
                self.close_menu();
            }
            return;
        }

        if self.app_state == AppState::Capturing {
            match self.input_mode {
                InputMode::Ascii => match key_event.code {
                    KeyCode::Char(c) => {
                        self.input_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                    }
                    KeyCode::Enter => {
                        self.send_input();
                    }
                    _ => {}
                },
                InputMode::Hex => match key_event.code {
                    KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                        self.input_buffer.push(c.to_ascii_uppercase());
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                    }
                    KeyCode::Enter => {
                        self.send_input();
                    }
                    _ => {}
                },
            }
        } else {
            self.handle_menu_key(key_event);
        }
    }

    fn open_menu(&mut self) {
        self.config_snapshot = Some(self.serial_config.clone());
        self.menu_cursor = 1; // first selectable item
        self.app_state = AppState::Options;
    }

    fn close_menu(&mut self) {
        if self.connection_status == ConnectionStatus::Connected
            && let Some(snapshot) = self.config_snapshot.take()
            && self.serial_config != snapshot
        {
            self.try_connect();
        }
        self.config_snapshot = None;
        self.app_state = AppState::Capturing;
    }

    fn handle_menu_key(&mut self, key_event: KeyEvent) {
        match self.app_state {
            AppState::Options => match key_event.code {
                KeyCode::Esc => {
                    self.close_menu();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_cursor_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_cursor_down();
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.cycle_current_item(false);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.cycle_current_item(true);
                }
                KeyCode::Enter => {
                    self.execute_current_item();
                }
                KeyCode::Char('x') => self.exit(),
                _ => {}
            },
            AppState::PortList => match key_event.code {
                KeyCode::Esc => {
                    self.app_state = AppState::Options;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.port_list_index = self.port_list_index.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !self.available_ports.is_empty() {
                        self.port_list_index =
                            (self.port_list_index + 1).min(self.available_ports.len() - 1);
                    }
                }
                KeyCode::Enter => {
                    if let Some(port) = self.available_ports.get(self.port_list_index) {
                        self.serial_config.port_info = Some(port.clone());
                        self.try_connect();
                        self.config_snapshot = None;
                        self.app_state = AppState::Capturing;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn move_cursor_up(&mut self) {
        let items = self.build_menu_items();
        let mut pos = self.menu_cursor;
        loop {
            if pos == 0 {
                return;
            }
            pos -= 1;
            if items[pos].is_selectable() {
                self.menu_cursor = pos;
                return;
            }
        }
    }

    fn move_cursor_down(&mut self) {
        let items = self.build_menu_items();
        let mut pos = self.menu_cursor;
        loop {
            pos += 1;
            if pos >= items.len() {
                return;
            }
            if items[pos].is_selectable() {
                self.menu_cursor = pos;
                return;
            }
        }
    }

    fn cycle_current_item(&mut self, forward: bool) {
        let items = self.build_menu_items();
        let Some(item) = items.get(self.menu_cursor) else {
            return;
        };
        let action = match item {
            MenuItem::Cycle { action, .. } => action.clone(),
            _ => return,
        };
        self.execute_cycle(&action, forward);
    }

    fn execute_current_item(&mut self) {
        let items = self.build_menu_items();
        let Some(item) = items.get(self.menu_cursor) else {
            return;
        };
        match item {
            MenuItem::Action { action, .. } => {
                let action = action.clone();
                self.execute_action(&action);
            }
            MenuItem::Cycle { action, .. } => {
                let action = action.clone();
                self.execute_cycle(&action, true);
            }
            _ => {}
        }
    }

    fn execute_action(&mut self, action: &MenuAction) {
        match action {
            MenuAction::SelectPort => {
                self.port_list_index = 0;
                self.app_state = AppState::PortList;
            }
            MenuAction::Disconnect => {
                self.disconnect();
                self.config_snapshot = None;
                self.app_state = AppState::Capturing;
            }
            MenuAction::Exit => self.exit(),
            _ => {}
        }
    }

    fn execute_cycle(&mut self, action: &MenuAction, forward: bool) {
        match action {
            MenuAction::CycleBaudRate => {
                let current = BAUD_RATES
                    .iter()
                    .position(|&b| b == self.serial_config.baud)
                    .unwrap_or(8); // default to 115200's index
                let next = if forward {
                    (current + 1) % BAUD_RATES.len()
                } else {
                    (current + BAUD_RATES.len() - 1) % BAUD_RATES.len()
                };
                self.serial_config.baud = BAUD_RATES[next];
            }
            MenuAction::CycleDataBits => {
                use serialport::DataBits::*;
                let options = [Five, Six, Seven, Eight];
                let current = options
                    .iter()
                    .position(|&d| d == self.serial_config.data_bits)
                    .unwrap_or(3);
                let next = if forward {
                    (current + 1) % options.len()
                } else {
                    (current + options.len() - 1) % options.len()
                };
                self.serial_config.data_bits = options[next];
            }
            MenuAction::CycleParity => {
                use serialport::Parity::*;
                let options = [None, Odd, Even];
                let current = options
                    .iter()
                    .position(|&p| p == self.serial_config.parity)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % options.len()
                } else {
                    (current + options.len() - 1) % options.len()
                };
                self.serial_config.parity = options[next];
            }
            MenuAction::CycleStopBits => {
                use serialport::StopBits::*;
                let options = [One, Two];
                let current = options
                    .iter()
                    .position(|&s| s == self.serial_config.stop_bits)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % options.len()
                } else {
                    (current + options.len() - 1) % options.len()
                };
                self.serial_config.stop_bits = options[next];
            }
            MenuAction::CycleFlowControl => {
                use serialport::FlowControl::*;
                let options = [None, Hardware, Software];
                let current = options
                    .iter()
                    .position(|&f| f == self.serial_config.flow_control)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % options.len()
                } else {
                    (current + options.len() - 1) % options.len()
                };
                self.serial_config.flow_control = options[next];
            }
            MenuAction::CycleLocalEcho => {
                self.local_echo = !self.local_echo;
            }
            MenuAction::CycleLineEnding => {
                let options = [
                    LineEnding::CrLf,
                    LineEnding::Cr,
                    LineEnding::Lf,
                    LineEnding::None,
                ];
                let current = options
                    .iter()
                    .position(|&l| l == self.line_ending)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % options.len()
                } else {
                    (current + options.len() - 1) % options.len()
                };
                self.line_ending = options[next];
            }
            MenuAction::CycleInputMode => {
                self.input_mode = match self.input_mode {
                    InputMode::Ascii => InputMode::Hex,
                    InputMode::Hex => InputMode::Ascii,
                };
                self.input_buffer.clear();
            }
            _ => {}
        }
    }

    fn try_connect(&mut self) {
        if let Some(handle) = self.serial_handle.take() {
            handle.disconnect();
        }

        match serial::connect(&self.serial_config) {
            Ok(handle) => {
                self.serial_handle = Some(handle);
                self.connection_status = ConnectionStatus::Connected;
                self.port_error = None;
                self.output_buffer.push_str("[Connected]\n");
            }
            Err(e) => {
                self.connection_status = ConnectionStatus::Disconnected;
                self.port_error = Some(e);
            }
        }
    }

    fn disconnect(&mut self) {
        if let Some(handle) = self.serial_handle.take() {
            handle.disconnect();
        }
        self.connection_status = ConnectionStatus::Disconnected;
        self.output_buffer.push_str("[Disconnected]\n");
    }

    fn drain_serial_events(&mut self) {
        let Some(handle) = self.serial_handle.as_ref() else {
            return;
        };

        loop {
            match handle.event_rx.try_recv() {
                Ok(SerialEvent::Data(data)) => {
                    self.output_buffer
                        .push_str(&String::from_utf8_lossy(&data));
                }
                Ok(SerialEvent::Error(msg)) => {
                    self.output_buffer
                        .push_str(&format!("[Error: {}]\n", msg));
                }
                Ok(SerialEvent::Disconnected) => {
                    self.serial_handle = None;
                    self.connection_status = ConnectionStatus::Disconnected;
                    self.output_buffer.push_str("[Disconnected]\n");
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.serial_handle = None;
                    self.connection_status = ConnectionStatus::Disconnected;
                    self.output_buffer.push_str("[Connection lost]\n");
                    return;
                }
            }
        }
    }

    fn send_input(&mut self) {
        if self.input_buffer.is_empty() {
            return;
        }

        match self.input_mode {
            InputMode::Ascii => {
                if let Some(handle) = self.serial_handle.as_ref() {
                    let mut data = self.input_buffer.clone().into_bytes();
                    data.extend_from_slice(self.line_ending.bytes());
                    let _ = handle.command_tx.send(SerialCommand::Send(data));
                }
                if self.local_echo {
                    self.output_buffer.push_str(&self.input_buffer);
                    self.output_buffer.push('\n');
                }
                self.input_buffer.clear();
            }
            InputMode::Hex => {
                let hex_str: String = self
                    .input_buffer
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit())
                    .collect();

                if !hex_str.len().is_multiple_of(2) {
                    self.output_buffer
                        .push_str("[Error: Odd number of hex digits]\n");
                    return;
                }

                let bytes: Vec<u8> = (0..hex_str.len())
                    .step_by(2)
                    .filter_map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16).ok())
                    .collect();

                if let Some(handle) = self.serial_handle.as_ref() {
                    let _ = handle
                        .command_tx
                        .send(SerialCommand::Send(bytes.clone()));
                }
                if self.local_echo {
                    let hex_display: Vec<String> =
                        bytes.iter().map(|b| format!("{:02X}", b)).collect();
                    self.output_buffer
                        .push_str(&format!("[TX: {}]\n", hex_display.join(" ")));
                }
                self.input_buffer.clear();
            }
        }
    }

    pub fn hex_display_buffer(&self) -> String {
        let chars: Vec<char> = self.input_buffer.chars().collect();
        let mut result = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if i > 0 && i % 2 == 0 {
                result.push(' ');
            }
            result.push(c);
        }
        result
    }

    fn scan_ports(&mut self) {
        match serialport::available_ports() {
            Ok(ports) => {
                self.available_ports = ports;
            }
            Err(e) => {
                self.available_ports.clear();
                self.port_error = Some(e.to_string());
            }
        }
    }

    fn exit(&mut self) {
        if let Some(handle) = self.serial_handle.take() {
            handle.disconnect();
        }
        self.exit = true;
    }
}
