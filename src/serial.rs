use serialport::{DataBits, FlowControl, Parity, SerialPortInfo, StopBits};

#[allow(dead_code)]
#[derive(Debug)]
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
