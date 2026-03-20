use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io;

use ratatui::DefaultTerminal;

use crate::serial::SerialConfig;
use crate::{Args, ui};

#[derive(Debug, PartialEq, Eq)]
pub enum AppState {
    Capturing,
    Options,
    PortList,
}

#[derive(Debug)]
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
            !todo!() // try to connect
        } else {
            self.app_state = AppState::Options;
        }
        while !self.exit {
            match serialport::available_ports() {
                Ok(ports) => {
                    self.available_ports = ports;
                    self.port_error = None;
                }
                Err(e) => {
                    self.available_ports.clear();
                    self.port_error = Some(e.to_string());
                }
            }
            terminal.draw(|frame| ui::draw(self, frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> io::Result<()> {
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
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
                    // send to port
                    self.input_buffer.clear();
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
                        // try to connect
                        self.app_state = AppState::Capturing;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}
