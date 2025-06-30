use fi_slurm::prometheus::{Cluster, Resource, Grouping};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::Stylize,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, LegendPosition},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;

// --- Data Structures ---

struct App<'a> {
    current_view: AppView,
    cpu_by_account: ChartData<'a>,
    cpu_by_node: ChartData<'a>,
    gpu_by_type: ChartData<'a>,
    should_quit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppView {
    CpuByAccount,
    CpuByNode,
    GpuByType,
}

struct ChartData<'a> {
    title: &'a str,
    source_data: HashMap<String, Vec<u64>>,
    y_axis_bounds: [f64; 2],
    y_axis_title: &'a str,
}

// Main Application Logic

pub fn tui_execute() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new();
    run_app(&mut terminal, app)?;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Char('1') => app.current_view = AppView::CpuByAccount,
                    KeyCode::Char('2') => app.current_view = AppView::CpuByNode,
                    KeyCode::Char('3') => app.current_view = AppView::GpuByType,
                    KeyCode::Right | KeyCode::Char('l')=> app.next_view(),
                    KeyCode::Left | KeyCode::Char('h')=> app.prev_view(),
                    _ => {}
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// UI Drawing 

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(f.area());

    draw_tabs(f, chunks[0], app.current_view);
    
    let chart_data = match app.current_view {
        AppView::CpuByAccount => &app.cpu_by_account,
        AppView::CpuByNode => &app.cpu_by_node,
        AppView::GpuByType => &app.gpu_by_type,
    };
    
    draw_chart(f, chunks[1], chart_data);
}

fn draw_tabs(f: &mut Frame, area: Rect, current_view: AppView) {
    let titles: Vec<Span> = ["1: CPU by Account", "2: CPU by Node Type", "3: GPU by Type"]
        .iter()
        .map(|t| Span::from(*t))
        .collect();
    
    let selected_index = match current_view {
        AppView::CpuByAccount => 0,
        AppView::CpuByNode => 1,
        AppView::GpuByType => 2,
    };

    let tabs = ratatui::widgets::Tabs::new(titles)
        .block(Block::default().title("Views").borders(Borders::ALL))
        .select(selected_index)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    f.render_widget(tabs, area);
}

fn draw_chart<B: Backend>(f: &mut Frame, area: Rect, data: &ChartData) {
    let colors = [
        Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Magenta,
        Color::Cyan, Color::Gray, Color::LightRed, Color::LightGreen, Color::LightYellow,
        Color::LightBlue,
    ];

    // --- FIX: Data Transformation for Stacked Chart ---
    // We need to calculate the cumulative sums before creating the datasets.
    
    // Sort the series by name to have a consistent stacking order.
    let mut sorted_series: Vec<_> = data.source_data.iter().collect();
    sorted_series.sort_by_key(|(name, _)| *name);

    // This will hold the y-values of the line below the current one,
    // allowing us to "stack" them. It starts at all zeros.
    let mut baseline: Vec<f64> = vec![0.0; 8]; // Assuming 8 data points
    
    let datasets: Vec<Dataset> = sorted_series
        .iter()
        .enumerate()
        .map(|(i, (name, values))| {
            let mut stacked_points = Vec::with_capacity(values.len());
            for (day_index, &value) in values.iter().enumerate() {
                // The new y-value is this series' value plus the baseline.
                let stacked_y = baseline[day_index] + value as f64;
                stacked_points.push((day_index as f64, stacked_y));
                // Update the baseline for the *next* series.
                baseline[day_index] = stacked_y;
            }

            Dataset::default()
                .name((*name).to_string())
                // FIX: Use GraphType::Line to connect the points.
                .graph_type(GraphType::Line)
                .style(Style::default().fg(colors[i % colors.len()]))
                .data(&stacked_points)
        })
        .collect();
        
    let x_axis = Axis::default()
        .title(Span::from("Time (Days Ago)"))
        .style(Style::default().fg(Color::Gray))
        .bounds([0.0, 7.0])
        .labels(
            ["-7d", "-6d", "-5d", "-4d", "-3d", "-2d", "-1d", "Today"]
                .iter()
                .cloned()
                .map(Span::from)
                .collect::<Vec<Span>>(),
        );

    let y_axis = Axis::default()
        .title(Span::from(data.y_axis_title))
        .style(Style::default().fg(Color::Gray))
        .bounds(data.y_axis_bounds)
        .labels(
            [
                data.y_axis_bounds[0],
                (data.y_axis_bounds[0] + data.y_axis_bounds[1]) / 2.0,
                data.y_axis_bounds[1],
            ]
            .iter()
            .map(|&v| Span::from(format!("{:.0}", v)))
            .collect::<Vec<Span>>(),
        );

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Span::from(data.title).bold())
                .borders(Borders::ALL),
        )
        .x_axis(x_axis)
        .y_axis(y_axis)
        .legend_position(Some(LegendPosition::TopRight));

    f.render_widget(chart, area);
}


// fn draw_chart(f: &mut Frame, area: Rect, data: &ChartData) {
//     let colors = [
//         Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Magenta,
//         Color::Cyan, Color::Gray, Color::LightRed, Color::LightGreen, Color::LightYellow,
//         Color::LightBlue,
//     ];
//
//     let all_series_data: Vec<(&String, Vec<(f64, f64)>)>  = data.source_data.iter().map(|(name, values)| {
//         let data_points: Vec<(f64, f64)> = values
//             .iter()
//             .enumerate()
//             .map(|(day_index, &value)| (day_index as f64, value as f64))
//             .collect();
//         (name, data_points)
//     }).collect();
//
//     let datasets: Vec<Dataset> = all_series_data.iter().enumerate().map(| (i, (name, data_points))| {
//         Dataset::default()
//             .name((*name).to_string())
//             .marker(symbols::Marker::Dot)
//             // do this as summative lines
//             .style(Style::default().fg(colors[i % colors.len()]))
//             .data(data_points)
//     }).collect();
//
//     let x_axis = Axis::default()
//         .title(Span::from("Time (Days Ago)"))
//         .style(Style::default().fg(Color::Gray))
//         .bounds([0.0, 7.0])
//         .labels(
//             ["-7d", "-6d", "-5d", "-4d", "-3d", "-2d", "-1d", "Today"]
//                 .iter()
//                 .cloned()
//                 .map(Span::from)
//                 .collect::<Vec<Span>>(),
//         );
//
//     let y_axis = Axis::default()
//         .title(Span::from(data.y_axis_title))
//         .style(Style::default().fg(Color::Gray))
//         .bounds(data.y_axis_bounds)
//         .labels(
//             [
//                 data.y_axis_bounds[0],
//                 (data.y_axis_bounds[0] + data.y_axis_bounds[1]) / 2.0,
//                 data.y_axis_bounds[1],
//             ]
//             .iter()
//             .map(|&v| Span::from(format!("{:.0}", v)))
//             .collect::<Vec<Span>>(),
//         );
//
//     let chart = Chart::new(datasets)
//         .block(
//             Block::default()
//                 .title(Span::from(data.title).bold())
//                 .borders(Borders::ALL),
//         )
//         .x_axis(x_axis)
//         .y_axis(y_axis)
//         .legend_position(Some(LegendPosition::TopRight))
//         .style(Style::default());
//
//     f.render_widget(chart, area);
// }


// App State and Data Loading

impl<'a> App<'a> {
    fn new() -> App<'a> {
        let cpu_by_account = get_cpu_by_account_data();
        let cpu_by_node = get_cpu_by_node_data();
        let gpu_by_type = get_gpu_by_type_data();

        App {
            current_view: AppView::CpuByAccount,
            cpu_by_account,
            cpu_by_node,
            gpu_by_type,
            should_quit: false,
        }
    }

    fn next_view(&mut self) {
        self.current_view = match self.current_view {
            AppView::CpuByAccount => AppView::CpuByNode,
            AppView::CpuByNode => AppView::GpuByType,
            AppView::GpuByType => AppView::CpuByAccount,
        }
    }

    fn prev_view(&mut self) {
        self.current_view = match self.current_view {
            AppView::CpuByAccount => AppView::GpuByType,
            AppView::CpuByNode => AppView::CpuByAccount,
            AppView::GpuByType => AppView::CpuByNode,
        }
    }
}

// Prometheus interface 

fn get_cpu_by_account_data<'a>() -> ChartData<'a> {
    let data = fi_slurm::prometheus::get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d").unwrap();

    let binding = data.clone();
    let max = binding.values().max().unwrap().iter().max().unwrap();
    // got to be a better way to get the max value from a hashmap of vectors of numbers
    
    ChartData {
        title: "CPU Usage by Account (8 Days)",
        source_data: data,
        y_axis_bounds: [0.0, *max as f64 * 1.2],
        y_axis_title: "CPU Cores",
    }
}

fn get_cpu_by_node_data<'a>() -> ChartData<'a> {
    let data = fi_slurm::prometheus::get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d").unwrap();
    
    let binding = data.clone();
    let max = binding.values().max().unwrap().iter().max().unwrap();

    ChartData {
        title: "CPU Usage by Node Type (8 Days)",
        source_data: data,
        y_axis_bounds: [0.0, *max as f64 * 1.2],
        y_axis_title: "CPU Cores",
    }
}

fn get_gpu_by_type_data<'a>() -> ChartData<'a> {
    let data = fi_slurm::prometheus::get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d").unwrap();
    
    let binding = data.clone();
    let max = binding.values().max().unwrap().iter().max().unwrap();

    ChartData {
        title: "GPU Usage by Type (8 Days)",
        source_data: data,
        y_axis_bounds: [0.0, *max as f64 * 1.2],
        y_axis_title: "GPUs",
    }
}
