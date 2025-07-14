use crate::tui::app::{App, AppError, AppState, AppView, ChartData};
use ratatui::{
    prelude::*,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Tabs, Wrap},
    Frame,
};

// --- UI Drawing ---

pub fn ui(f: &mut Frame, app_state: &AppState) {
    match app_state {
        AppState::Loading { tick } => draw_loading_screen(f, *tick),
        AppState::Loaded(app) => draw_dashboard(f, app),
        AppState::Error(err) => draw_error_screen(f, err),
    }
}

fn draw_loading_screen(f: &mut Frame, tick: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(45),
                Constraint::Length(3),
                Constraint::Percentage(45),
            ]
            .as_ref(),
        )
        .split(f.area());

    let loading_text = "Loading Data";
    let dots = ".".repeat(tick % 4);
    let text = format!("{}{}", loading_text, dots);

    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Status")
                .border_set(border::ROUNDED),
        )
        .alignment(Alignment::Center);

    f.render_widget(paragraph, chunks[1]);
}

fn draw_error_screen(f: &mut Frame, err: &AppError) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(40),
                Constraint::Min(5),
                Constraint::Percentage(40),
            ]
            .as_ref(),
        )
        .split(f.area());

    let error_text = Text::from(vec![
        Line::from(Span::styled(
            "An error occurred:",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(err.to_string()),
        Line::from(""),
        Line::from("Press 'q' to quit."),
    ]);

    let paragraph = Paragraph::new(error_text)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Error")
                .border_style(Style::default().fg(Color::Red))
                .border_set(border::ROUNDED),
        )
        .alignment(Alignment::Center);

    f.render_widget(paragraph, chunks[1]);
}

fn draw_dashboard(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // For tabs
                Constraint::Min(0),    // For chart content
                Constraint::Length(1), // For footer
            ]
            .as_ref(),
        )
        .split(f.area());

    draw_tabs(f, main_chunks[0], app.current_view);

    let chart_data = match app.current_view {
        AppView::CpuByAccount => &app.cpu_by_account,
        AppView::CpuByNode => &app.cpu_by_node,
        AppView::GpuByType => &app.gpu_by_type,
    };

    let page_info = draw_charts(f, main_chunks[1], chart_data, app.scroll_offset);
    draw_footer(f, main_chunks[2], Some(page_info));
}

fn draw_tabs(f: &mut Frame, area: Rect, current_view: AppView) {
    let titles: Vec<Line> = ["(1) CPU by Account", "(2) CPU by Node", "(3) GPU by Type"]
        .iter()
        .map(|t| Line::from(t.bold()))
        .collect();

    let selected_index = match current_view {
        AppView::CpuByAccount => 0,
        AppView::CpuByNode => 1,
        AppView::GpuByType => 2,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title("Dashboard Views")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .select(selected_index)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue),
        );

    f.render_widget(tabs, area);
}

fn draw_charts(f: &mut Frame, area: Rect, data: &ChartData, scroll_offset: usize) -> (usize, usize) {
    // --- Layout Constants ---
    const MINIMUM_CHART_WIDTH: u16 = 70;
    const CHART_HEIGHT: u16 = 10;
    const BAR_WIDTH: u16 = 6;
    const BAR_GAP: u16 = 1;

    // --- Data Preparation ---
    let colors = [
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::Green,
        Color::Red,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightYellow,
        Color::LightGreen,
        Color::LightRed,
    ];
    let time_labels = ["-7d", "-6d", "-5d", "-4d", "-3d", "-2d", "-1d", "Now"];

    let mut sorted_series: Vec<_> = data.source_data.iter().collect();
    sorted_series.sort_by_key(|(name, _)| *name);

    // --- Grid Calculation ---
    let num_charts = sorted_series.len();
    if num_charts == 0 {
        return (1,1);
    }

    let num_cols = (area.width / MINIMUM_CHART_WIDTH).max(1) as usize;
    let total_rows = num_charts.div_ceil(num_cols);

    let num_visible_rows = (area.height / CHART_HEIGHT) as usize;
    let max_scroll_offset = total_rows.saturating_sub(num_visible_rows);
    let clamped_offset = scroll_offset.min(max_scroll_offset);
    // --- Create Row Layouts ---
    let row_constraints = vec![Constraint::Length(CHART_HEIGHT); num_visible_rows];
    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    // --- Iterate and Draw Each Chart in a Grid ---
    let mut chart_iter = sorted_series.iter().skip(clamped_offset * num_cols);
    for i in 0..num_visible_rows {
        if i >= row_chunks.len() {
            break;
        }
        let row_area = row_chunks[i];

        // --- Create Column Layouts for the current row ---
        let col_constraints = vec![Constraint::Percentage(100 / num_cols as u16); num_cols];
        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for j in 0..num_cols {
            if j >= col_chunks.len() {
                break;
            }
            if let Some((name, values)) = chart_iter.next() {
                let cell_area = col_chunks[j];

                let outer_block = Block::default()
                    .title(Span::from(*name).bold())
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED);

                let inner_area = outer_block.inner(cell_area);
                f.render_widget(outer_block, cell_area);

                let chart_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1), // For usage numbers
                        Constraint::Min(0),    // For the bar chart
                    ])
                    .split(inner_area);
                let labels_area = chart_chunks[0];
                let chart_area = chart_chunks[1];

                // ensuring that colors are consistent when scrolling
                let absolute_chart_index = (clamped_offset + i) * num_cols + j;
                let color = colors[absolute_chart_index % colors.len()];

                // --- Create Bars (with original values) ---
                let mut bar_data: Vec<Bar> = values
                    .iter()
                    .enumerate()
                    .map(|(k, &val)| {
                        Bar::default()
                            .value(val)
                            .label(time_labels[k % time_labels.len()].into())
                            .style(Style::default().fg(color))
                            .text_value("".to_string())
                    })
                    .collect();

                // Get the specific capacity vector for this chart.
                let capacity_vec = data.capacity_data.get(*name);
                
                // Find the maximum capacity for this specific chart.
                let chart_specific_max = capacity_vec.and_then(|v| v.iter().max()).cloned().unwrap_or(0);

                // Add an invisible bar with this chart's specific max capacity.
                bar_data.push(
                    Bar::default()
                        .value(chart_specific_max)
                        .label("".into())
                        .style(Style::default().add_modifier(Modifier::HIDDEN)),
                );


                // --- Draw Usage Labels (with original values) ---
                let label_constraints: Vec<Constraint> = (0..values.len())
                    .flat_map(|_| [Constraint::Length(BAR_WIDTH), Constraint::Length(BAR_GAP)])
                    .collect();

                let label_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(label_constraints)
                    .split(labels_area);

                for (k, &val) in values.iter().enumerate() {
                    let label_chunk_index = k * 2;
                    if label_chunk_index < label_chunks.len() {
                        let label = Paragraph::new(val.to_string())
                            .style(Style::default().fg(Color::White))
                            .alignment(Alignment::Center);
                        f.render_widget(label, label_chunks[label_chunk_index]);
                    }
                }

                // --- Render the BarChart ---
                let bar_group = BarGroup::default().bars(&bar_data);
                let barchart = BarChart::default()
                    .data(bar_group)
                    .bar_width(BAR_WIDTH)
                    .bar_gap(BAR_GAP);

                f.render_widget(barchart, chart_area);
            }
        }
    }

    // Return the current page and total pages for the footer.
    let current_page = clamped_offset + 1;
    let total_pages = max_scroll_offset + 1;
    (current_page, total_pages)
}

fn draw_footer(f: &mut Frame, area: Rect, page_info: Option<(usize, usize)>) {
    let base_text = "Use (q) to quit, (h/l, ←/→, Tab, or numbers) to switch views.";
    
    let footer_text = if let Some((current, total)) = page_info {
        if total > 1 {
            format!("{} (k/j, ↑/↓ to scroll) Page {} / {}", base_text, current, total)
        } else {
            base_text.to_string()
        }
    } else {
        base_text.to_string()
    };

    let footer_paragraph = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::White).bg(Color::DarkGray))
        .alignment(Alignment::Center);
        
    f.render_widget(footer_paragraph, area);
}
