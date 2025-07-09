use fi_prometheus::{Cluster, Resource, Grouping, get_usage_by, get_max_resource};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;
use tokio::sync::mpsc;

// --- Data Structures ---

#[derive(Error, Debug, Clone)]
pub enum AppError {
    #[error("Failed to fetch data from source: {0}")]
    DataFetch(String),
    #[error("A background task failed: {0}")]
    TaskJoin(String),
    #[error("Failed to send data to UI thread: {0}")]
    ChannelSend(String),
}

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

#[allow(clippy::large_enum_variant)]
enum AppState<'a> {
    Loading { tick: usize },
    Loaded(App<'a>),
    Error(AppError),
}

#[derive(Debug)]
enum FetchedData<'a> {
    CpuByAccount(Result<ChartData<'a>, AppError>),
    CpuByNode(Result<ChartData<'a>, AppError>),
    GpuByType(Result<ChartData<'a>, AppError>),
}

#[derive(Debug)]
enum FetchedCapacity {
    CpuByAccount(ChartData),
    CpuByNode(ChartData),
    GpuByType(ChartData),
}

#[derive(Debug)]
struct ChartData<'a> {
    _title: &'a str,
    source_data: HashMap<String, Vec<u64>>,
    _y_axis_bounds: [f64; 2],
    _y_axis_title: &'a str,
}

struct ChartCapacity {
    capacity_vec: HashMap<String, u64>,
    max_capacity: u64,
}

#[tokio::main]
pub async fn tui_execute() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel(3);

    tokio::spawn(get_cpu_by_account_data_async(tx.clone()));
    tokio::spawn(get_cpu_by_node_data_async(tx.clone()));
    tokio::spawn(get_gpu_by_type_data_async(tx.clone()));

    let res = run_app(&mut terminal, rx).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error in app: {:?}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    mut rx: mpsc::Receiver<FetchedData<'_>>,
) -> io::Result<()> {

    let mut app_state = AppState::Loading {tick: 0};

    let mut cpu_by_account_data = None;
    let mut cpu_by_node_data = None;
    let mut gpu_by_type_data = None;


    loop {
        terminal.draw(|f| ui(f, &app_state))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {

                if key.code == KeyCode::Char('q') {
                    if let AppState::Loaded(ref mut app) = app_state {
                        app.should_quit = true;
                    } else {
                        return Ok(())
                    }
                }

                if let AppState::Loaded(ref mut app) = app_state {
                    match key.code {
                        KeyCode::Char('1') => app.current_view = AppView::CpuByAccount,
                        KeyCode::Char('2') => app.current_view = AppView::CpuByNode,
                        KeyCode::Char('3') => app.current_view = AppView::GpuByType,
                        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => app.next_view(),
                        KeyCode::Left | KeyCode::Char('h') => app.prev_view(),
                        _ => {}
                    }
                }
            }
        }

        if let Ok(fetched_data) = rx.try_recv() {
            match fetched_data {
                FetchedData::CpuByAccount(data) => cpu_by_account_data = Some(data),
                FetchedData::CpuByNode(data) => cpu_by_node_data = Some(data),
                FetchedData::GpuByType(data) => gpu_by_type_data = Some(data),
            }
        }

        if let AppState::Loading {ref mut tick} = app_state {
            *tick += 1;

            if cpu_by_account_data.is_some() && cpu_by_node_data.is_some() && gpu_by_type_data.is_some() {
                let app = App {
                    current_view: AppView::CpuByAccount,
                    cpu_by_account: cpu_by_account_data.take().unwrap(),
                    cpu_by_node: cpu_by_node_data.take().unwrap(),
                    gpu_by_type: gpu_by_type_data.take().unwrap(),
                    should_quit: false,
                };
                app_state = AppState::Loaded(app);
            }
        }

        if let AppState::Loaded(app) = &app_state {
            if app.should_quit {
                return Ok(());
            }
        }
    }
}

// --- UI Drawing ---

// MODIFIED: The main UI function now dispatches based on AppState.
fn ui(f: &mut Frame, app_state: &AppState) {
    match app_state {
        AppState::Loading { tick } => draw_loading_screen(f, *tick),
        AppState::Loaded(app) => draw_dashboard(f, app),
        AppState::Error(err) => draw_error_screen(f, err),
    }
}

// NEW: A function to draw the loading screen.
fn draw_loading_screen(f: &mut Frame, tick: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(3),
            Constraint::Percentage(45),
        ].as_ref())
        .split(f.area());

    let loading_text = "Loading Data";
    let dots = ".".repeat(tick % 4);
    let text = format!("{}{}", loading_text, dots);

    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Status").border_set(border::ROUNDED))
        .alignment(Alignment::Center);

    f.render_widget(paragraph, chunks[1]);
}

fn draw_error_screen(f: &mut Frame, err: &AppError) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Min(5),
            Constraint::Percentage(40),
        ].as_ref())
        .split(f.area());

    let error_text = Text::from(vec![
        Line::from(Span::styled("An error occurred:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(err),
        Line::from(""),
        Line::from("Press 'q' to quit."),
    ]);

    let paragraph = Paragraph::new(error_text)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Error").border_style(Style::default().fg(Color::Red)).border_set(border::ROUNDED))
        .alignment(Alignment::Center);

    f.render_widget(paragraph, chunks[1]);
}


// NEW: Renamed from `ui` to `draw_dashboard` to be more specific.
fn draw_dashboard(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // For tabs
            Constraint::Min(0),    // For chart content
            Constraint::Length(1), // For footer
        ].as_ref())
        .split(f.area());

    draw_tabs(f, main_chunks[0], app.current_view);

    let chart_data = match app.current_view {
        AppView::CpuByAccount => &app.cpu_by_account,
        AppView::CpuByNode => &app.cpu_by_node,
        AppView::GpuByType => &app.gpu_by_type,
    };

    draw_charts(f, main_chunks[1], chart_data);
    draw_footer(f, main_chunks[2]);
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
        .block(Block::default().title("Dashboard Views").borders(Borders::ALL).border_style(Style::default().fg(Color::White)))
        .select(selected_index)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue),
        );

    f.render_widget(tabs, area);
}

fn draw_charts(f: &mut Frame, area: Rect, data: &ChartData) {
    // --- Layout Constants ---
    const DESIRED_CHART_WIDTH: u16 = 50;
    const CHART_HEIGHT: u16 = 10;
    const BAR_WIDTH: u16 = 4;
    const BAR_GAP: u16 = 1;

    // --- Data Preparation ---
    let colors = [
        Color::Cyan, Color::Magenta, Color::Yellow, Color::Green, Color::Red,
        Color::LightBlue, Color::LightMagenta, Color::LightYellow, Color::LightGreen, Color::LightRed,
    ];
    let time_labels = ["-7d", "-6d", "-5d", "-4d", "-3d", "-2d", "-1d", "Now"];

    let mut sorted_series: Vec<_> = data.source_data.iter().collect();
    sorted_series.sort_by_key(|(name, _)| *name);

    // --- Grid Calculation ---
    let num_charts = sorted_series.len();
    if num_charts == 0 { return; }

    let num_cols = (area.width / DESIRED_CHART_WIDTH).max(1) as usize;
    let num_rows = num_charts.div_ceil(num_cols);

    // --- Create Row Layouts ---
    let row_constraints = vec![Constraint::Length(CHART_HEIGHT); num_rows];
    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    // --- Iterate and Draw Each Chart in a Grid ---
    let mut chart_iter = sorted_series.iter();
    for i in 0..num_rows {
        if i >= row_chunks.len() { break; }
        let row_area = row_chunks[i];
        
        // --- Create Column Layouts for the current row ---
        let col_constraints = vec![Constraint::Percentage(100 / num_cols as u16); num_cols];
        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for j in 0..num_cols {
            if j >= col_chunks.len() { break; }
            if let Some((name, values)) = chart_iter.next() {
                let cell_area = col_chunks[j];

                // --- NEW: Split the cell into a top area for labels and a bottom for the chart ---
                let chart_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(0)])
                    .split(cell_area);
                let labels_area = chart_chunks[0];
                let chart_area = chart_chunks[1];

                // --- Create Bars (with no text value) ---
                let bar_data: Vec<Bar> = values
                    .iter()
                    .enumerate()
                    .map(|(k, &val)| {
                        Bar::default()
                            .value(val) // Set numeric value for scaling
                            .label(time_labels[k % time_labels.len()].into())
                            .style(Style::default().fg(colors[(i * num_cols + j) % colors.len()]))
                            // --- CHANGED: Render an empty string on the bar itself ---
                            .text_value("".to_string())
                    })
                    .collect();

                // --- NEW: Manually render labels in the top area ---
                let mut label_constraints = Vec::new();
                for _ in 0..values.len() {
                    label_constraints.push(Constraint::Length(BAR_WIDTH));
                    label_constraints.push(Constraint::Length(BAR_GAP));
                }
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

                // --- Render the BarChart in the bottom area ---
                let bar_group = BarGroup::default().bars(&bar_data);
                let barchart = BarChart::default()
                    .block(
                        Block::default()
                            .title(Span::from(*name).bold())
                            .border_set(border::ROUNDED)
                    )
                    .data(bar_group)
                    .bar_width(BAR_WIDTH)
                    .bar_gap(BAR_GAP);
                
                f.render_widget(barchart, chart_area);
            }
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let footer_text = "Use (q) to quit, (h/l, ←/→, Tab, or numbers) to switch views.";
    let footer = Block::default()
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    f.render_widget(footer, area);
    f.render_widget(Line::from(footer_text).alignment(Alignment::Center), area);
}

// --- App State and Data Loading ---

impl App<'_> {
    //fn new() -> App<'a> {
    //    let cpu_by_account = get_cpu_by_account_data();
    //    let cpu_by_node = get_cpu_by_node_data();
    //    let gpu_by_type = get_gpu_by_type_data();
    //
    //    App {
    //        current_view: AppView::CpuByAccount,
    //        cpu_by_account,
    //        cpu_by_node,
    //        gpu_by_type,
    //        should_quit: false,
    //    }
    //}

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

// --- Prometheus interface ---

// Prometheus interface 

fn get_cpu_by_account_data<'a>() -> ChartData<'a> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d").unwrap_or_default();

    let binding = data.clone();
    let max = binding.values().map(|vec| vec.iter().sum::<u64>()).max().unwrap_or(0);
    
    ChartData {
        _title: "CPU Usage by Account (8 Days)",
        source_data: data,
        _y_axis_bounds: [0.0, max as f64],
        _y_axis_title: "CPU Cores",
    }
}


fn get_cpu_capacity_by_account() -> ChartCapacity {
    let data = get_max_resource(Cluster::Rusty, Some(Grouping::Account), Resource::Cpus, Some(7), Some("1d")).unwrap_or_default();

    let binding = data.clone();
    let max = binding.values().max().unwrap_or(0);

    ChartCapacity {
        capacity_vec: data,
        max_capacity: max,
    }
}

async fn get_cpu_capacity_by_account_async(tx: mpsc::Sender<FetchedCapacity>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_capacity_by_account()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedCapacity::CpuByAccount(data)).await.is_err() {
            // Handle error: receiver was dropped.
        }
    }
}

fn get_cpu_by_node_data<'a>() -> ChartData<'a> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d").unwrap_or_default();
    
    let binding = data.clone();
    let max = binding.values().map(|vec| vec.iter().sum::<u64>()).max().unwrap_or(0);

    ChartData {
        _title: "CPU Usage by Node Type (8 Days)",
        source_data: data,
        _y_axis_bounds: [0.0, max as f64],
        _y_axis_title: "CPU Cores",
    }
}


fn get_cpu_capacity_by_node() -> ChartCapacity {
    let data = get_max_resource(Cluster::Rusty, Some(Grouping::Nodes), Resource::Cpus, Some(7), Some("1d")).unwrap_or_default();

    let binding = data.clone();
    let max = binding.values().max().unwrap_or(0);

    ChartCapacity {
        capacity_vec: data,
        max_capacity: max,
    }
}

fn get_gpu_by_type_data<'a>() -> ChartData<'a> {
    let data = get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d").unwrap_or_default();
    
    let binding = data.clone();
    let max = binding.values().map(|vec| vec.iter().sum::<u64>()).max().unwrap_or(0);

    ChartData {
        _title: "GPU Usage by Type (8 Days)",
        source_data: data,
        _y_axis_bounds: [0.0, max as f64],
        _y_axis_title: "GPUs",
    }
}

fn get_gpu_capacity_by_type() -> ChartCapacity {
    let data = get_max_resource(Cluster::Rusty, Some(Grouping::GpuType), Resource::Cpus, Some(7), Some("1d")).unwrap_or_default();

    let binding = data.clone();
    let max = binding.values().max().unwrap_or(0);

    ChartCapacity {
        capacity_vec: data,
        max_capacity: max,
    }
}


async fn get_cpu_by_account_data_async(tx: mpsc::Sender<FetchedData<'_>>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_by_account_data()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedData::CpuByAccount(Ok(data))).await.is_err() {
            // Handle error: receiver was dropped.
        }
    }
}

async fn get_cpu_by_node_data_async(tx: mpsc::Sender<FetchedData<'_>>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_by_node_data()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedData::CpuByNode(Ok(data))).await.is_err() {
            // Handle error
        }
    }
}

async fn get_gpu_by_type_data_async(tx: mpsc::Sender<FetchedData<'_>>) {
    let result = tokio::task::spawn_blocking(move || {
        get_gpu_by_type_data()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedData::GpuByType(Ok(data))).await.is_err() {
            // Handle error
        }
    }
}

