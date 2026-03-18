use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::io;

use ratatui::DefaultTerminal;

use crate::serial::SerialConfig;
use crate::{Args, ui};

#[derive(Debug)]
pub struct App {
    pub args: Args,
    pub exit: bool,
    pub serial_config: SerialConfig,
    pub capturing_input: bool,
    pub input_buffer: String,
}

impl App {
    pub fn new(args: Args) -> Self {
        Self {
            args,
            exit: false,
            serial_config: SerialConfig::default(),
            capturing_input: false,
            input_buffer: String::new(),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
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
        if self.capturing_input {
            // append to buffer
        } else {
            // handle app input
            match key_event.code {
                KeyCode::Char('q') => self.exit(),
                _ => {}
            }
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}
