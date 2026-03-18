use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::App;

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
}

fn render_output(_app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::new();
    let paragraph =
        Paragraph::new(Line::from(vec![Span::from(_app.args.baud.to_string())])).block(block);
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
    let port_string = app.serial_config.port.as_deref().unwrap_or(" - ");
    let connected = app.serial_config.port.is_some();
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
