use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::io::Write;

use crate::app::{App, AppState, ConnectionStatus, InputMode, MenuItem, Selection, SelectionMode};

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

pub fn draw(app: &mut App, frame: &mut Frame) {
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

fn render_output(app: &mut App, frame: &mut Frame, area: Rect) {
    app.last_output_height = area.height;
    app.last_output_area = area;

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

    app.recompute_heights_for_width(area.width);

    let (pending_line, pending_plain) = if app.output_pending.is_empty() {
        (Line::default(), String::new())
    } else {
        crate::app::parse_ansi_line(&app.output_pending)
    };
    let pending_height = if pending_plain.is_empty() {
        0_u16
    } else {
        crate::app::compute_line_height(&pending_plain, area.width)
    };

    let total_visual_lines = app.total_visual_lines();
    let max_offset = total_visual_lines.saturating_sub(area.height);

    let scroll = if app.auto_scroll {
        max_offset
    } else {
        app.scroll_top = app.scroll_top.min(max_offset);
        app.scroll_top
    };

    app.last_total_visual_lines = total_visual_lines;
    app.last_scroll = scroll;

    let viewport_end = (scroll as u32) + (area.height as u32);

    let mut acc: u32 = 0;
    let mut start_idx: usize = app.output_lines.len() + 1;
    let mut start_offset: u16 = 0;
    let mut end_idx: usize = app.output_lines.len();
    let has_pending = !pending_plain.is_empty();
    let total_count = app.output_lines.len() + if has_pending { 1 } else { 0 };

    for i in 0..total_count {
        let h = if i < app.output_lines.len() {
            app.output_lines[i].height as u32
        } else {
            pending_height as u32
        };
        let line_top = acc;
        let line_bottom = acc + h;

        if start_idx > app.output_lines.len() && line_bottom > scroll as u32 {
            start_idx = i;
            start_offset = (scroll as u32 - line_top).min(u16::MAX as u32) as u16;
        }
        if start_idx <= app.output_lines.len() && line_top >= viewport_end {
            end_idx = i;
            break;
        }
        acc = line_bottom;
        if i == total_count - 1 {
            end_idx = total_count;
        }
    }

    let mut visible_lines: Vec<Line> = Vec::with_capacity(end_idx.saturating_sub(start_idx));
    if start_idx <= app.output_lines.len() {
        for i in start_idx..end_idx {
            let line = if i < app.output_lines.len() {
                app.output_lines[i].line.clone()
            } else {
                pending_line.clone()
            };
            visible_lines.push(line);
        }
    }

    let paragraph = Paragraph::new(visible_lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph.scroll((start_offset, 0)), area);

    apply_selection(app, frame, area);
}

fn apply_selection(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(sel) = app.selection else {
        return;
    };

    let buf = frame.buffer_mut();
    let scroll = app.last_scroll;

    let Some((start, end)) = compute_selection_range(&sel, area, scroll, &*buf) else {
        if app.copy_pending {
            app.copy_pending = false;
        }
        return;
    };

    let text_to_copy = if app.copy_pending && should_copy(&sel) {
        Some(extract_selected_text(area, start, end, scroll, &*buf))
    } else {
        None
    };

    let viewport_end = scroll.saturating_add(area.height);
    let visible_start_vline = start.1.max(scroll);
    let visible_end_vline = end.1.min(viewport_end.saturating_sub(1));
    let max_col = area.width.saturating_sub(1);

    if visible_start_vline <= visible_end_vline && area.height > 0 && area.width > 0 {
        for vline in visible_start_vline..=visible_end_vline {
            let screen_y = area.y + (vline - scroll);
            let col_start = if vline == start.1 { start.0.min(max_col) } else { 0 };
            let col_end = if vline == end.1 { end.0.min(max_col) } else { max_col };
            for col in col_start..=col_end {
                let screen_x = area.x + col;
                if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                    let new_style = cell.style().add_modifier(Modifier::REVERSED);
                    cell.set_style(new_style);
                }
            }
        }
    }

    if app.copy_pending {
        app.copy_pending = false;
        if let Some(text) = text_to_copy
            && !text.is_empty()
        {
            let _ = copy_to_clipboard(&text);
        }
    }
}

fn should_copy(sel: &Selection) -> bool {
    !(sel.mode == SelectionMode::Char && sel.anchor == sel.cursor)
}

fn vline_to_screen_y(vline: u16, scroll: u16, area: Rect) -> Option<u16> {
    if vline < scroll {
        return None;
    }
    let rel = vline - scroll;
    if rel >= area.height {
        return None;
    }
    Some(area.y + rel)
}

fn compute_selection_range(
    sel: &Selection,
    area: Rect,
    scroll: u16,
    buf: &Buffer,
) -> Option<((u16, u16), (u16, u16))> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let (a, c) = (sel.anchor, sel.cursor);
    // Order by (vline, col)
    let (start, end) = if (a.1, a.0) <= (c.1, c.0) {
        (a, c)
    } else {
        (c, a)
    };
    match sel.mode {
        SelectionMode::Char => Some((start, end)),
        SelectionMode::Line => Some(((0, start.1), (area.width.saturating_sub(1), end.1))),
        SelectionMode::Word => Some((
            expand_to_word_start(start, area, scroll, buf),
            expand_to_word_end(end, area, scroll, buf),
        )),
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn cell_first_char(buf: &Buffer, x: u16, y: u16) -> Option<char> {
    buf.cell((x, y))?.symbol().chars().next()
}

fn expand_to_word_start(pos: (u16, u16), area: Rect, scroll: u16, buf: &Buffer) -> (u16, u16) {
    let (mut col, vline) = pos;
    let Some(screen_y) = vline_to_screen_y(vline, scroll, area) else {
        return pos;
    };
    let max_col = area.width.saturating_sub(1);
    if col > max_col {
        return pos;
    }
    if !cell_first_char(buf, area.x + col, screen_y).is_some_and(is_word_char) {
        return pos;
    }
    while col > 0 {
        if !cell_first_char(buf, area.x + col - 1, screen_y).is_some_and(is_word_char) {
            break;
        }
        col -= 1;
    }
    (col, vline)
}

fn expand_to_word_end(pos: (u16, u16), area: Rect, scroll: u16, buf: &Buffer) -> (u16, u16) {
    let (mut col, vline) = pos;
    let Some(screen_y) = vline_to_screen_y(vline, scroll, area) else {
        return pos;
    };
    let max_col = area.width.saturating_sub(1);
    if col > max_col {
        return pos;
    }
    if !cell_first_char(buf, area.x + col, screen_y).is_some_and(is_word_char) {
        return pos;
    }
    while col < max_col {
        if !cell_first_char(buf, area.x + col + 1, screen_y).is_some_and(is_word_char) {
            break;
        }
        col += 1;
    }
    (col, vline)
}

fn extract_selected_text(
    area: Rect,
    start: (u16, u16),
    end: (u16, u16),
    scroll: u16,
    buf: &Buffer,
) -> String {
    let mut result = String::new();
    let max_col = area.width.saturating_sub(1);
    let viewport_end = scroll.saturating_add(area.height);
    let visible_start = start.1.max(scroll);
    let visible_end = end.1.min(viewport_end.saturating_sub(1));

    if visible_start > visible_end {
        return result;
    }

    for vline in visible_start..=visible_end {
        let screen_y = area.y + (vline - scroll);
        let col_start = if vline == start.1 {
            start.0.min(max_col)
        } else {
            0
        };
        let col_end = if vline == end.1 {
            end.0.min(max_col)
        } else {
            max_col
        };
        let mut line = String::new();
        for col in col_start..=col_end {
            let screen_x = area.x + col;
            if let Some(cell) = buf.cell((screen_x, screen_y)) {
                line.push_str(cell.symbol());
            }
        }
        result.push_str(line.trim_end());
        if vline < visible_end {
            result.push('\n');
        }
    }
    result
}

fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    let encoded = base64_encode(text.as_bytes());
    let seq = format!("\x1b]52;c;{}\x07", encoded);
    let mut stdout = std::io::stdout();
    stdout.write_all(seq.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        result.push(CHARS[(b0 >> 2) as usize] as char);
        result.push(CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
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

    if !app.auto_scroll {
        spans.push(Span::styled(
            " PAUSED ",
            Style::default().fg(Color::Yellow).bold(),
        ));
        spans.push(Span::from(" "));
    }

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
