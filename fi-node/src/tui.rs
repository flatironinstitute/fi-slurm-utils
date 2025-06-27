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

// FIX: ChartData now owns the source data directly.
struct ChartData<'a> {
    title: &'a str,
    source_data: HashMap<&'a str, Vec<u64>>,
    y_axis_bounds: [f64; 2],
    y_axis_title: &'a str,
}

// --- Main Application Logic ---

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        terminal.draw(|f| ui::<B>(f, &app))?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Char('1') => app.current_view = AppView::CpuByAccount,
                    KeyCode::Char('2') => app.current_view = AppView::CpuByNode,
                    KeyCode::Char('3') => app.current_view = AppView::GpuByType,
                    KeyCode::Right => app.next_view(),
                    KeyCode::Left => app.prev_view(),
                    _ => {}
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// --- UI Drawing ---

fn ui<B: Backend>(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(f.area());

    draw_tabs::<B>(f, chunks[0], app.current_view);
    
    let chart_data = match app.current_view {
        AppView::CpuByAccount => &app.cpu_by_account,
        AppView::CpuByNode => &app.cpu_by_node,
        AppView::GpuByType => &app.gpu_by_type,
    };
    
    draw_chart::<B>(f, chunks[1], chart_data);
}

fn draw_tabs<B: Backend>(f: &mut Frame, area: Rect, current_view: AppView) {
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
    // FIX: Datasets are now created "just-in-time" inside the draw function.
    // This solves the lifetime issue because the `all_data_points` Vec
    // lives for the entire duration of this function call.
    let colors = [
        Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Magenta,
        Color::Cyan, Color::Gray, Color::LightRed, Color::LightGreen, Color::LightYellow,
        Color::LightBlue,
    ];

    let datasets: Vec<Dataset> = data
        .source_data
        .iter()
        .enumerate()
        .map(|(i, (name, values))| {
            let data_points: Vec<(f64, f64)> = values
                .iter()
                .enumerate()
                .map(|(day_index, &value)| (day_index as f64, value as f64))
                .collect();

            Dataset::default()
                .name((*name).to_string())
                .marker(symbols::Marker::Dot)
                .style(Style::default().fg(colors[i % colors.len()]))
                .data(&data_points) // Borrows the data from the local `data_points`
        })
        .collect();
        
    let x_axis = Axis::default()
        .title(Span::from("Time (Days Ago)"))
        .style(Style::default().fg(Color::Gray))
        .bounds([0.0, 7.0]) // 8 days total
        .labels::<Vec<Span>>(
            ["-7d", "-6d", "-5d", "-4d", "-3d", "-2d", "-1d", "Today"]
                .iter()
                .cloned()
                .map(Span::from)
                .collect(),
        );

    let y_axis = Axis::default()
        .title(Span::from(data.y_axis_title))
        .style(Style::default().fg(Color::Gray))
        .bounds(data.y_axis_bounds)
        .labels::<Vec<Span>>(
            [
                data.y_axis_bounds[0],
                (data.y_axis_bounds[0] + data.y_axis_bounds[1]) / 2.0,
                data.y_axis_bounds[1],
            ]
            .iter()
            .map(|&v| Span::from(format!("{:.0}", v)))
            .collect(),
        );

    let chart = Chart::new(datasets) // Use the newly created datasets
        .block(
            Block::default()
                .title(Span::from(data.title).bold())
                .borders(Borders::ALL),
        )
        .x_axis(x_axis)
        .y_axis(y_axis)
        .legend_position(Some(LegendPosition::TopRight))
        .style(Style::default().fg(Color::White));

    f.render_widget(chart, area);
}


// --- App State and Data Loading ---

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


// --- Hardcoded Sample Data ---
// FIX: These functions now return ChartData structs that own the source data.

fn get_cpu_by_account_data<'a>() -> ChartData<'a> {
    let mut data: HashMap<&str, Vec<u64>> = HashMap::new();
    data.insert("scc", vec![320, 96, 0, 0, 0, 0, 0, 0]);
    data.insert("cca", vec![47088, 55076, 49644, 47153, 53669, 47712, 47059, 51621]);
    data.insert("ccq", vec![13069, 15037, 13427, 8736, 6113, 14145, 11137, 11903]);
    data.insert("cmbas", vec![3305, 3317, 3141, 13541, 30837, 34459, 13595, 13297]);
    
    ChartData {
        title: "CPU Usage by Account (8 Days)",
        source_data: data,
        y_axis_bounds: [0.0, 150000.0],
        y_axis_title: "CPU Cores",
    }
}

fn get_cpu_by_node_data<'a>() -> ChartData<'a> {
    let mut data: HashMap<&str, Vec<u64>> = HashMap::new();
    data.insert("icelake", vec![12726, 12480, 12295, 10590, 12930, 10053, 12922, 12832]);
    data.insert("rome", vec![32838, 75145, 65599, 60634, 76185, 73253, 43232, 55127]);
    data.insert("genoa", vec![26592, 40704, 35232, 29760, 38432, 33184, 30628, 39636]);
    
    ChartData {
        title: "CPU Usage by Node Type (8 Days)",
        source_data: data,
        y_axis_bounds: [0.0, 150000.0],
        y_axis_title: "CPU Cores",
    }
}

fn get_gpu_by_type_data<'a>() -> ChartData<'a> {
    let mut data: HashMap<&str, Vec<u64>> = HashMap::new();
    data.insert("a100-sxm4-80gb", vec![85, 111, 82, 105, 93, 77, 108, 93]);
    data.insert("h100_pcie", vec![77, 91, 94, 67, 102, 81, 109, 94]);
    data.insert("a100-sxm4-40gb", vec![25, 63, 37, 36, 67, 82, 98, 97]);
    
    ChartData {
        title: "GPU Usage by Type (8 Days)",
        source_data: data,
        y_axis_bounds: [0.0, 150.0],
        y_axis_title: "GPUs",
    }
} 
