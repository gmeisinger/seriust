use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serialport::{DataBits, FlowControl, Parity, SerialPortInfo, StopBits};

// Messages from worker thread -> App
pub enum SerialEvent {
    Data(Vec<u8>),
    Error(String),
    Disconnected,
}

// Commands from App -> worker thread
pub enum SerialCommand {
    Send(Vec<u8>),
    Disconnect,
}

pub struct SerialHandle {
    pub event_rx: mpsc::Receiver<SerialEvent>,
    pub command_tx: mpsc::Sender<SerialCommand>,
    worker_thread: Option<thread::JoinHandle<()>>,
}

impl SerialHandle {
    pub fn disconnect(mut self) {
        let _ = self.command_tx.send(SerialCommand::Disconnect);
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct SerialConfig {
    pub port_info: Option<SerialPortInfo>,
    pub baud: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port_info: None,
            baud: 115200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
        }
    }
}

pub fn connect(config: &SerialConfig) -> Result<SerialHandle, String> {
    let port_info = config
        .port_info
        .as_ref()
        .ok_or_else(|| "No port selected".to_string())?;

    let port = serialport::new(&port_info.port_name, config.baud)
        .data_bits(config.data_bits)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .flow_control(config.flow_control)
        .timeout(Duration::from_millis(10))
        .open()
        .map_err(|e| e.to_string())?;

    let (event_tx, event_rx) = mpsc::channel();
    let (command_tx, command_rx) = mpsc::channel();

    let worker_thread = thread::spawn(move || {
        serial_worker(port, event_tx, command_rx);
    });

    Ok(SerialHandle {
        event_rx,
        command_tx,
        worker_thread: Some(worker_thread),
    })
}

fn serial_worker(
    mut port: Box<dyn serialport::SerialPort>,
    event_tx: mpsc::Sender<SerialEvent>,
    command_rx: mpsc::Receiver<SerialCommand>,
) {
    let mut buf = [0u8; 1024];

    loop {
        match command_rx.try_recv() {
            Ok(SerialCommand::Send(data)) => {
                if let Err(e) = port.write_all(&data).and_then(|_| port.flush()) {
                    let _ = event_tx.send(SerialEvent::Error(e.to_string()));
                }
            }
            Ok(SerialCommand::Disconnect) => {
                let _ = event_tx.send(SerialEvent::Disconnected);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => return,
            Err(mpsc::TryRecvError::Empty) => {}
        }

        match port.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                if event_tx.send(SerialEvent::Data(buf[..n].to_vec())).is_err() {
                    return;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(ref e)
                if e.kind() == std::io::ErrorKind::BrokenPipe
                    || e.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                let _ = event_tx.send(SerialEvent::Error(e.to_string()));
                let _ = event_tx.send(SerialEvent::Disconnected);
                return;
            }
            Err(e) => {
                let _ = event_tx.send(SerialEvent::Error(e.to_string()));
            }
        }
    }
}
