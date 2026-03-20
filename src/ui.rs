use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{App, AppState};

pub fn draw(app: &App, frame: &mut Frame) {
    // outer shell
    let instructions = Line::from(vec![" <CTRL+A> ".bold(), " OPTIONS ".into()]);
    let outer = Block::bordered()
        .title(" Seriust ")
        .title_bottom(instructions.right_aligned())
        .border_set(border::PLAIN);
    let inner = outer.inner(frame.area());
    frame.render_widget(outer, frame.area());

    let [output_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .areas(inner);

    // Render each inner area
    render_output(app, frame, output_area);
    render_input(app, frame, input_area);
    render_status(app, frame, status_area);

    // Merging borders manually
    let area = frame.area();
    let buf = frame.buffer_mut();
    buf.set_string(area.x, input_area.y, "├", Style::default());
    buf.set_string(area.x + area.width - 1, input_area.y, "┤", Style::default());
    buf.set_string(area.x, status_area.y, "├", Style::default());
    buf.set_string(
        area.x + area.width - 1,
        status_area.y,
        "┤",
        Style::default(),
    );

    if app.app_state != AppState::Capturing {
        match app.app_state {
            AppState::Options => render_menu(frame),
            AppState::PortList => render_port_list(app, frame),
            _ => {}
        }
    }
}

fn render_output(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::new();
    let paragraph = if let Some(err) = &app.port_error {
        Paragraph::new(Line::from(Span::from(err.as_str()).red()))
            .block(block)
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new(format!("{}", app.output_buffer))
            .block(block)
            .wrap(Wrap { trim: false })
    };
    frame.render_widget(paragraph, area);
}

fn render_input(app: &App, frame: &mut Frame, area: Rect) {
    let input_prompt = Line::from(vec![
        Span::from("> "),
        Span::from(app.input_buffer.to_string()),
    ]);
    let block = Block::new().borders(Borders::TOP).border_set(border::PLAIN);
    let paragraph = Paragraph::new(input_prompt).block(block);
    frame.render_widget(paragraph, area);
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let connected = app.serial_config.port_info.is_some();
    let port_string = if let Some(info) = &app.serial_config.port_info {
        info.port_name.clone()
    } else {
        " - ".to_string()
    };
    let baud_string = if connected {
        app.serial_config.baud.to_string()
    } else {
        " - ".to_string()
    };
    let indicator_span = if connected {
        Span::from(" ● ").green()
    } else {
        Span::from(" ● ").red()
    };
    let connected_string = if connected {
        " Connected "
    } else {
        " Disconnected "
    };
    let status_line = Line::from(vec![
        Span::from(port_string).bold(),
        Span::from(baud_string).dark_gray(),
        indicator_span,
        Span::from(connected_string),
    ]);
    let block = Block::new().borders(Borders::TOP).border_set(border::PLAIN);
    let paragraph = Paragraph::new(status_line).block(block);
    frame.render_widget(paragraph, area);
}

fn render_menu(frame: &mut Frame) {
    let area = frame.area();
    let popup_width = 40.min(area.width.saturating_sub(4));
    let popup_height = 10.min(area.height.saturating_sub(4));
    let popup_x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let menu_items = vec![
        Line::from("  [P] Select Port"),
        Line::from("  [B] Change Baud Rate"),
        Line::from("  [X] Exit"),
        Line::from(""),
        Line::from("  Press ESC to close"),
    ];
    let menu_block = Block::bordered()
        .title(" Options ")
        .border_set(border::DOUBLE);
    let menu = Paragraph::new(menu_items).block(menu_block);
    frame.render_widget(menu, popup_area);
}

fn render_port_list(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let list_width = 40.min(area.width.saturating_sub(4));
    let list_height = (app.available_ports.len() as u16 + 2).min(area.height.saturating_sub(4));
    let list_x = (area.width.saturating_sub(list_width)) / 2 + area.x;
    let list_y = (area.height.saturating_sub(list_height)) / 2 + area.y;
    let list_area = Rect::new(list_x, list_y, list_width, list_height);

    frame.render_widget(Clear, list_area);

    let indicator_width = 2; // "● " or "  "
    let inner_width = list_width.saturating_sub(2) as usize; // account for borders
    let items: Vec<Line> = app
        .available_ports
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let type_label = match &p.port_type {
                serialport::SerialPortType::UsbPort(_) => "USB",
                serialport::SerialPortType::PciPort => "PCI",
                serialport::SerialPortType::BluetoothPort => "Bluetooth",
                serialport::SerialPortType::Unknown => "Unknown",
            };
            let is_connected = app.serial_config.port_info.is_some();
            let indicator = if is_connected {
                Span::from("● ").green()
            } else {
                Span::from("  ")
            };
            let name = &p.port_name;
            let padding =
                inner_width.saturating_sub(name.len() + type_label.len() + indicator_width);
            let mut line = Line::from(vec![
                indicator,
                Span::from(name.as_str()),
                Span::from(" ".repeat(padding)),
                Span::from(type_label).dark_gray(),
            ]);
            if i == app.port_list_index {
                line = line.style(Style::default().reversed());
            }
            line
        })
        .collect();
    let list = Paragraph::new(items).block(Block::bordered());
    frame.render_widget(list, list_area);
}
