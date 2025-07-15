use crate::tui::app::{App, AppError, AppState, AppView, ChartData, MainMenuSelection, 
    ParameterFocus, ParameterSelectionState, ScrollMode};
use ratatui::{
    prelude::*,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Tabs, Wrap},
    Frame,
};

use fi_prometheus::PrometheusTimeScale;

// --- Layout Constants ---
const MINIMUM_CHART_WIDTH: u16 = 65;
const CHART_HEIGHT: u16 = 10;
pub const MAX_BARS_PER_CHART: usize = 10;
const BAR_WIDTH: u16 = 6;
const BAR_GAP: u16 = 1;
// --- UI Drawing ---

pub fn ui(f: &mut Frame, app_state: &AppState) {
    match app_state {
        AppState::MainMenu { selected } => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(f.area());
            draw_main_menu(f, chunks[0], *selected);
            draw_footer(f, chunks[1], None, None);
        }
        AppState::ParameterSelection(state) => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(f.area());
            draw_parameter_selection_menu(f, chunks[0], state);
            draw_footer(f, chunks[1], None, Some(state.focused_widget));
        }
        AppState::Loading { tick } => draw_loading_screen(f, *tick),
        AppState::Loaded(app) => {
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // For tabs
                    Constraint::Min(0),    // For chart content
                    Constraint::Length(1), // For footer
                ])
                .split(f.area());

            draw_tabs(f, main_chunks[0], app.current_view, None);
            let page_info = draw_charts(f, main_chunks[1], get_chart_data(app), app.scroll_offset, app.query_time_scale, app.scroll_mode);
            // Pass page info to both tabs and footer
            draw_tabs(f, main_chunks[0], app.current_view, Some(page_info));
            draw_footer(f, main_chunks[2], Some(page_info), None);
        }
        AppState::Error(err) => draw_error_screen(f, err),
    }
}


fn draw_main_menu(f: &mut Frame, area: Rect, selected: MainMenuSelection) {
    // Create a layout to center the menu box.
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(5),
            Constraint::Percentage(40),
        ])
        .split(area);

    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
        .split(vertical_chunks[1]);
    
    let menu_area = horizontal_chunks[1];

    let selected_style = Style::default().bg(Color::Blue).fg(Color::White);
    let normal_style = Style::default().fg(Color::White);

    let default_text = Paragraph::new("View Default Dashboard (Last 7 Days)")
        .alignment(Alignment::Center)
        .style(if selected == MainMenuSelection::Default { selected_style } else { normal_style });
    
    let custom_text = Paragraph::new("Custom Query")
        .alignment(Alignment::Center)
        .style(if selected == MainMenuSelection::Custom { selected_style } else { normal_style });

    let block = Block::default()
        .title("Prometheus TUI")
        .borders(Borders::ALL)
        .border_set(border::ROUNDED);
    
    let inner_area = block.inner(menu_area);
    f.render_widget(block, menu_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1), // Spacer
            Constraint::Length(1),
        ])
        .split(inner_area);

    f.render_widget(default_text, inner_chunks[0]);
    f.render_widget(custom_text, inner_chunks[2]);
}

fn draw_parameter_selection_menu(f: &mut Frame, area: Rect, state: &ParameterSelectionState) {
    // Main layout with a chunk for the menu and a chunk for the debug pane.
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(5), // Space for the debug pane
        ])
        .split(area);
    
    let menu_area = main_chunks[0];

    // --- Draw the original menu ---
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(9),
            Constraint::Percentage(35),
        ])
        .split(menu_area);

    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical_chunks[1]);

    let inner_menu_area = horizontal_chunks[1];

    let main_block = Block::default()
        .title("Custom Query Parameters")
        .borders(Borders::ALL)
        .border_set(border::ROUNDED);
    let inner_area = main_block.inner(inner_menu_area);
    f.render_widget(main_block, inner_menu_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner_area);

    let focused_style = Style::default().fg(Color::Yellow);
    let normal_style = Style::default().fg(Color::White);

    let range_block = Block::default()
        .title("Range")
        .borders(Borders::ALL)
        .border_style(if state.focused_widget == ParameterFocus::Range { focused_style } else { normal_style });
    
    let input_text = if state.focused_widget == ParameterFocus::Range {
        format!("{}█", state.range_input)
    } else {
        state.range_input.clone()
    };
    let range_paragraph = Paragraph::new(input_text).block(range_block);
    f.render_widget(range_paragraph, inner_chunks[0]);

    let unit_block = Block::default()
        .title("Unit")
        .borders(Borders::ALL)
        .border_style(if state.focused_widget == ParameterFocus::Unit { focused_style } else { normal_style });

    let unit_time = match state.selected_unit {
        PrometheusTimeScale::Minutes => "Minutes",
        PrometheusTimeScale::Hours => "Hours",
        PrometheusTimeScale::Days => "Days",
        PrometheusTimeScale::Weeks => "Weeks",
        PrometheusTimeScale::Years => "Years",
    };
    let unit_text = format!("< {} >", unit_time);
    let unit_paragraph = Paragraph::new(unit_text).block(unit_block).alignment(Alignment::Center);
    f.render_widget(unit_paragraph, inner_chunks[1]);

    let confirm_text = "Confirm";
    let confirm_paragraph = Paragraph::new(confirm_text)
        .alignment(Alignment::Center)
        .style(if state.focused_widget == ParameterFocus::Confirm { focused_style.add_modifier(Modifier::REVERSED) } else { normal_style });
    f.render_widget(confirm_paragraph, inner_chunks[2]);
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

// NEW: Helper function to get the current chart data.
fn get_chart_data(app: &App) -> &ChartData {
    match app.current_view {
        AppView::CpuByAccount => &app.cpu_by_account,
        AppView::CpuByNode => &app.cpu_by_node,
        AppView::GpuByType => &app.gpu_by_type,
    }
}

fn draw_tabs(f: &mut Frame, area: Rect, current_view: AppView, page_info: Option<(usize, usize)>) {
    let base_titles = ["(1) CPU by Account", "(2) CPU by Node", "(3) GPU by Type"];
    
    let selected_index = match current_view {
        AppView::CpuByAccount => 0,
        AppView::CpuByNode => 1,
        AppView::GpuByType => 2,
    };

    let titles: Vec<Line> = base_titles
        .iter()
        .enumerate()
        .map(|(i, &title)| {
            let title_str = if i == selected_index {
                if let Some((current, total)) = page_info {
                    if total > 1 {
                        format!("{} ({}/{})", title, current, total)
                    } else {
                        title.to_string()
                    }
                } else {
                    title.to_string()
                }
            } else {
                title.to_string()
            };
            Line::from(title_str.bold())
        })
        .collect();

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

fn draw_charts(f: &mut Frame, area: Rect, data: &ChartData, scroll_offset: usize, _time_scale: PrometheusTimeScale, scroll_mode: ScrollMode) -> (usize, usize) {

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

                let border_style = if scroll_mode == ScrollMode::Chart {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                
                let outer_block = Block::default()
                    .title(Span::from(*name).bold())
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(border_style);

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

                let num_points = values.len();

                let max_h_scroll = num_points.saturating_sub(MAX_BARS_PER_CHART);
                let h_offset = data.horizontal_scroll_offset.min(max_h_scroll);

                let visible_values: Vec<_> = values.iter().skip(h_offset).take(MAX_BARS_PER_CHART).collect();

                let time_labels: Vec<String> = (h_offset..h_offset + visible_values.len()).map(|i| {
                    let step = num_points - 1 - i;
                    if step == 0 {
                        "Now".to_string()
                    } else {
                        format!("-{step}")
                    }
                }).collect();

                // --- Create Bars (with original values) ---
                let mut bar_data: Vec<Bar> = visible_values
                    .iter()
                    .enumerate()
                    .map(|(k, &val)| {
                        Bar::default()
                            .value(*val)
                            .label(time_labels[k % time_labels.len()].clone().into())
                            //.label(time_labels[k % time_labels.len()].into())
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
                let label_constraints: Vec<Constraint> = (0..visible_values.len())
                    .flat_map(|_| [Constraint::Length(BAR_WIDTH), Constraint::Length(BAR_GAP)])
                    .collect();

                let label_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(label_constraints)
                    .split(labels_area);

                for (k, &val) in visible_values.iter().enumerate() {
                    let label_chunk_index = k * 2;
                    if label_chunk_index < label_chunks.len() {
                        let label = Paragraph::new(val.to_string())
                            .style(Style::default().fg(Color::White))
                            .alignment(Alignment::Center);
                        f.render_widget(label, label_chunks[label_chunk_index]);
                    }
                }

                if h_offset > 0 {
                    f.render_widget(Paragraph::new("...").alignment(Alignment::Left), labels_area);
                }
                if h_offset < max_h_scroll {
                    f.render_widget(Paragraph::new("...").alignment(Alignment::Right), labels_area);
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

fn draw_footer(f: &mut Frame, area: Rect, page_info: Option<(usize, usize)>, focus: Option<ParameterFocus>) {
    let mut instructions = vec![Span::from("Use (q) to quit")];

    if let Some((_, total)) = page_info {
        instructions.push(Span::from(", (h/l, ←/→, Tab, or numbers) to switch views"));
        if total > 1 {
            instructions.push(Span::from(", (↑/↓ to scroll)"));
        }
        instructions.push(Span::from(", (Enter to scroll charts)"));
    } else if let Some(focus_widget) = focus {
        instructions.push(Span::from(", (Tab to switch focus)"));
        match focus_widget {
            ParameterFocus::Range => instructions.push(Span::from(", (Enter numbers)")),
            ParameterFocus::Unit => instructions.push(Span::from(", (←/→ to change)")),
            ParameterFocus::Confirm => instructions.push(Span::from(", (Enter to confirm)")),
        }
    } else {
        instructions.push(Span::from(", (↑/↓ to select), (Enter) to confirm"));
    }

    let footer_text = Line::from(instructions).alignment(Alignment::Center);

    let footer_paragraph = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
        
    f.render_widget(footer_paragraph, area);
}
