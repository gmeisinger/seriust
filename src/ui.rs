use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{App, AppState, ConnectionStatus, InputMode, MenuItem};

fn parity_char(p: serialport::Parity) -> char {
    match p {
        serialport::Parity::None => 'N',
        serialport::Parity::Odd => 'O',
        serialport::Parity::Even => 'E',
    }
}

fn data_bits_char(d: serialport::DataBits) -> char {
    match d {
        serialport::DataBits::Five => '5',
        serialport::DataBits::Six => '6',
        serialport::DataBits::Seven => '7',
        serialport::DataBits::Eight => '8',
    }
}

fn stop_bits_char(s: serialport::StopBits) -> char {
    match s {
        serialport::StopBits::One => '1',
        serialport::StopBits::Two => '2',
    }
}

pub fn draw(app: &App, frame: &mut Frame) {
    let title_style = if app.connection_status == ConnectionStatus::Connected {
        Style::default().fg(Color::Green).bold()
    } else {
        Style::default().bold()
    };
    let instructions = Line::from(vec![
        Span::styled(" Ctrl+A ", Style::default().fg(Color::DarkGray)),
        Span::styled("Options ", Style::default().fg(Color::DarkGray)),
    ]);
    let outer = Block::bordered()
        .title(Span::styled(" Seriust ", title_style))
        .title_bottom(instructions.right_aligned())
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = outer.inner(frame.area());
    frame.render_widget(outer, frame.area());

    let [output_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .areas(inner);

    render_output(app, frame, output_area);
    render_input(app, frame, input_area);
    render_status(app, frame, status_area);

    // Merging borders manually
    let area = frame.area();
    let border_style = Style::default().fg(Color::DarkGray);
    let buf = frame.buffer_mut();
    buf.set_string(area.x, input_area.y, "├", border_style);
    buf.set_string(area.x + area.width - 1, input_area.y, "┤", border_style);
    buf.set_string(area.x, status_area.y, "├", border_style);
    buf.set_string(area.x + area.width - 1, status_area.y, "┤", border_style);

    if app.app_state != AppState::Capturing {
        match app.app_state {
            AppState::Options => render_menu(app, frame),
            AppState::PortList => render_port_list(app, frame),
            _ => {}
        }
    }
}

fn render_output(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::new();

    if let Some(err) = &app.port_error {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
        return;
    }

    let lines: Vec<Line> = app
        .output_buffer
        .lines()
        .map(|l| Line::from(l.to_string()))
        .collect();

    let visible_height = area.height as usize;
    let scroll = if lines.len() > visible_height {
        (lines.len() - visible_height) as u16
    } else {
        0
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn render_input(app: &App, frame: &mut Frame, area: Rect) {
    let (prompt, display_text) = match app.input_mode {
        InputMode::Ascii => (
            Span::styled("> ", Style::default().fg(Color::DarkGray)),
            app.input_buffer.clone(),
        ),
        InputMode::Hex => (
            Span::styled("HEX> ", Style::default().fg(Color::Yellow).bold()),
            app.hex_display_buffer(),
        ),
    };

    let input_prompt = Line::from(vec![prompt, Span::from(display_text)]);
    let block = Block::new()
        .borders(Borders::TOP)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(Color::DarkGray));
    let paragraph = Paragraph::new(input_prompt).block(block);
    frame.render_widget(paragraph, area);
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let connected = app.connection_status == ConnectionStatus::Connected;
    let port_string = if let Some(info) = &app.serial_config.port_info {
        info.port_name.clone()
    } else {
        "—".to_string()
    };

    let config_shorthand = format!(
        " {:>6} {}{}{}",
        app.serial_config.baud,
        data_bits_char(app.serial_config.data_bits),
        parity_char(app.serial_config.parity),
        stop_bits_char(app.serial_config.stop_bits),
    );

    let indicator_span = if connected {
        Span::styled(" ● ", Style::default().fg(Color::Green))
    } else {
        Span::styled(" ● ", Style::default().fg(Color::Red))
    };

    let status_text = if connected {
        "Connected"
    } else {
        "Disconnected"
    };

    let mut spans = vec![
        Span::styled(config_shorthand, Style::default().fg(Color::DarkGray)),
        indicator_span,
        Span::from(format!("{:<12} ", status_text)),
    ];

    if app.input_mode == InputMode::Hex {
        spans.push(Span::styled(
            " HEX ",
            Style::default().fg(Color::Yellow).bold(),
        ));
    }

    spans.push(Span::styled(port_string, Style::default().bold()));

    let status_line = Line::from(spans);
    let block = Block::new()
        .borders(Borders::TOP)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(Color::DarkGray));
    let paragraph = Paragraph::new(status_line).block(block);
    frame.render_widget(paragraph, area);
}

fn render_menu(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let items = app.build_menu_items();

    // Each SectionHeader adds 2 lines (blank + header), others add 1, plus 1 trailing blank
    let section_count = items
        .iter()
        .filter(|i| matches!(i, MenuItem::SectionHeader(_)))
        .count() as u16;
    let content_height = items.len() as u16 + section_count + 1;
    let popup_width = 46_u16.min(area.width.saturating_sub(4));
    let popup_height = (content_height + 2).min(area.height.saturating_sub(4));
    let popup_x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let menu_block = Block::bordered()
        .title(Span::styled(" Options ", Style::default().bold()))
        .border_set(border::DOUBLE)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_width = popup_width.saturating_sub(2) as usize; // inside borders

    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let is_selected = i == app.menu_cursor;

        match item {
            MenuItem::SectionHeader(title) => {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  {}", title),
                    Style::default().fg(Color::Cyan).bold(),
                )));
            }
            MenuItem::Action { label, .. } => {
                let cursor = if is_selected { "  > " } else { "    " };
                let text = format!("{}{}", cursor, label);
                let padded = format!("{:<width$}", text, width = inner_width);
                let style = if is_selected {
                    Style::default().reversed()
                } else {
                    Style::default()
                };
                lines.push(Line::styled(padded, style));
            }
            MenuItem::Cycle { label, value, .. } => {
                let cursor = if is_selected { "  > " } else { "    " };
                let arrows_left = if is_selected { "◂ " } else { "  " };
                let arrows_right = if is_selected { " ▸" } else { "  " };
                let value_with_arrows = format!("{}{}{}", arrows_left, value, arrows_right);
                let label_part = format!("{}{}", cursor, label);
                let padding =
                    inner_width.saturating_sub(label_part.len() + value_with_arrows.len());
                let text = format!("{}{}{}", label_part, " ".repeat(padding), value_with_arrows);
                let style = if is_selected {
                    Style::default().reversed()
                } else {
                    Style::default()
                };
                lines.push(Line::styled(text, style));
            }
        }
    }

    lines.push(Line::from("")); // bottom padding

    let menu = Paragraph::new(lines).block(menu_block);
    frame.render_widget(menu, popup_area);
}

fn render_port_list(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let list_width = 44_u16.min(area.width.saturating_sub(4));
    let list_height = (app.available_ports.len() as u16 + 2).min(area.height.saturating_sub(4));
    let list_x = (area.width.saturating_sub(list_width)) / 2 + area.x;
    let list_y = (area.height.saturating_sub(list_height)) / 2 + area.y;
    let list_area = Rect::new(list_x, list_y, list_width, list_height);

    frame.render_widget(Clear, list_area);

    let indicator_width = 2;
    let inner_width = list_width.saturating_sub(2) as usize;
    let items: Vec<Line> = app
        .available_ports
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let type_label = match &p.port_type {
                serialport::SerialPortType::UsbPort(_) => "USB",
                serialport::SerialPortType::PciPort => "PCI",
                serialport::SerialPortType::BluetoothPort => "BT",
                serialport::SerialPortType::Unknown => "?",
            };
            let is_connected_port = app.connection_status == ConnectionStatus::Connected
                && app
                    .serial_config
                    .port_info
                    .as_ref()
                    .map(|info| info.port_name == p.port_name)
                    .unwrap_or(false);
            let indicator = if is_connected_port {
                Span::styled("● ", Style::default().fg(Color::Green))
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
                Span::styled(type_label, Style::default().fg(Color::DarkGray)),
            ]);
            if i == app.port_list_index {
                line = line.style(Style::default().reversed());
            }
            line
        })
        .collect();
    let list_block = Block::bordered()
        .title(Span::styled(" Select Port ", Style::default().bold()))
        .border_set(border::DOUBLE)
        .border_style(Style::default().fg(Color::DarkGray));
    let list = Paragraph::new(items).block(list_block);
    frame.render_widget(list, list_area);
}
