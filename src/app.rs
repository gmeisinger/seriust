use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;

use crate::serial::{self, SerialCommand, SerialConfig, SerialEvent, SerialHandle};
use crate::{Args, ui};

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
    serial_handle: Option<SerialHandle>,
    last_port_scan: Instant,
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
            serial_handle: None,
            last_port_scan: Instant::now(),
        }
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
                self.app_state = AppState::Options;
            } else {
                self.app_state = AppState::Capturing;
            }
            return;
        }

        // Menu is open -- consume all keys for menu navigation
        if self.app_state == AppState::Capturing {
            match key_event.code {
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
            }
        } else {
            self.handle_menu_key(key_event);
            return;
        }
    }

    fn handle_menu_key(&mut self, key_event: KeyEvent) {
        match self.app_state {
            AppState::Options => match key_event.code {
                KeyCode::Esc => {
                    self.app_state = AppState::Capturing;
                }
                KeyCode::Char('p') => {
                    self.port_list_index = 0;
                    self.app_state = AppState::PortList;
                }
                KeyCode::Char('d') => {
                    self.disconnect();
                    self.app_state = AppState::Capturing;
                }
                KeyCode::Char('e') => {
                    self.local_echo = !self.local_echo;
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
                        self.app_state = AppState::Capturing;
                    }
                }
                _ => {}
            },
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

        if let Some(handle) = self.serial_handle.as_ref() {
            let mut data = self.input_buffer.clone().into_bytes();
            data.push(b'\r');
            data.push(b'\n');
            let _ = handle.command_tx.send(SerialCommand::Send(data));
        }
        if self.local_echo {
            self.output_buffer.push_str(&self.input_buffer);
            self.output_buffer.push('\n');
        }
        self.input_buffer.clear();
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
