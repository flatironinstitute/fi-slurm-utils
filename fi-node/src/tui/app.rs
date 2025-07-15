use crate::tui::{
    interface::{
        get_cpu_by_account_data_async, get_cpu_by_node_data_async,
        get_gpu_by_type_data_async, get_cpu_capacity_by_account_async,
        get_cpu_capacity_by_node_async, get_gpu_capacity_by_type_async,
    },
    ui::{ui, MAX_BARS_PER_CHART}
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use fi_prometheus::PrometheusTimeScale;
use std::collections::HashMap;
use std::io;
use tokio::sync::mpsc;
use std::time::Duration;
use thiserror::Error;

// --- Data Structures ---

#[derive(Error, Debug, Clone)]
pub enum AppError {
    #[error("Failed to fetch data from source: {0}")]
    DataFetch(String),
    #[error("A background task failed: {0}")]
    TaskJoin(String),
    #[error("Failed to send data to UI thread: {0}")]
    ChannelSend(String),
    #[error("Failed to get maximum capacity: {0}")]
    MaxFail(String),
    #[error("Data fetching timed out after 10 seconds")]
    TimeOut,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    CpuByAccount,
    CpuByNode,
    GpuByType,
}

// #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum ScrollMode {
    #[default]
    Page,
    Chart,
}

#[derive(Debug)]
pub struct ChartData {
    pub source_data: HashMap<String, Vec<u64>>,
    pub capacity_data: HashMap<String, Vec<u64>>,
    pub horizontal_scroll_offset: usize,
}
pub struct App {
    pub current_view: AppView,
    pub scroll_offset: usize,
    pub scroll_mode: ScrollMode,
    pub cpu_by_account: ChartData,
    pub cpu_by_node: ChartData,
    pub gpu_by_type: ChartData,
    pub should_quit: bool,
    pub query_range: i64,
    pub query_time_scale: PrometheusTimeScale,
}

impl App {
    fn next_view(&mut self) {
        self.current_view = match self.current_view {
            AppView::CpuByAccount => AppView::CpuByNode,
            AppView::CpuByNode => AppView::GpuByType,
            AppView::GpuByType => AppView::CpuByAccount,
        };
        self.scroll_offset = 0;
    }

    fn prev_view(&mut self) {
        self.current_view = match self.current_view {
            AppView::CpuByAccount => AppView::GpuByType,
            AppView::CpuByNode => AppView::CpuByAccount,
            AppView::GpuByType => AppView::CpuByNode,
        };
        self.scroll_offset = 0;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MainMenuSelection {
    #[default]
    Default,
    Custom,
}

impl MainMenuSelection {
    pub fn toggle(&self) -> Self {
        match self {
            MainMenuSelection::Default => MainMenuSelection::Custom,
            MainMenuSelection::Custom => MainMenuSelection::Default,
        }
    }
}

// --- NEW: Structs and Enums for the Parameter Selection state ---
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ParameterFocus {
    #[default]
    Range,
    Unit,
    Confirm,
}

impl ParameterFocus {
    fn next(&self) -> Self {
        match self {
            ParameterFocus::Range => ParameterFocus::Unit,
            ParameterFocus::Unit => ParameterFocus::Confirm,
            ParameterFocus::Confirm => ParameterFocus::Range,
        }
    }
}

#[derive(Debug, Default)]
pub struct ParameterSelectionState {
    pub range_input: String,
    pub selected_unit: PrometheusTimeScale,
    pub focused_widget: ParameterFocus,
}


// MODIFIED: The AppState enum now includes all application states.
#[allow(clippy::large_enum_variant)]
//#[derive(Debug, Clone)]
pub enum AppState {
    MainMenu { selected: MainMenuSelection },
    ParameterSelection(ParameterSelectionState),
    Loading { tick: usize },
    Loaded(App),
    Error(AppError),
}

#[derive(Debug)]
pub struct UsageData {
    pub source_data: HashMap<String, Vec<u64>>,
}

#[derive(Debug)]
pub struct CapacityData {
    pub capacities: HashMap<String, Vec<u64>>,
}

#[derive(Debug)]
pub enum FetchedData {
    CpuByAccount(Result<UsageData, AppError>),
    CpuByNode(Result<UsageData, AppError>),
    GpuByType(Result<UsageData, AppError>),
    CpuCapacityByAccount(Result<CapacityData, AppError>),
    CpuCapacityByNode(Result<CapacityData, AppError>),
    GpuCapacityByType(Result<CapacityData, AppError>),
}

fn spawn_custom_data_fetch(tx: mpsc::Sender<FetchedData>, range: i64, unit: PrometheusTimeScale) {
    tokio::spawn(get_cpu_by_account_data_async(tx.clone(), range, unit));
    tokio::spawn(get_cpu_by_node_data_async(tx.clone(), range, unit));
    tokio::spawn(get_gpu_by_type_data_async(tx.clone(), range, unit));
    tokio::spawn(get_cpu_capacity_by_account_async(tx.clone(), range, unit));
    tokio::spawn(get_cpu_capacity_by_node_async(tx.clone(), range, unit));
    tokio::spawn(get_gpu_capacity_by_type_async(tx.clone(), range, unit));
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut rx: mpsc::Receiver<FetchedData>,
) -> io::Result<()> {

    const LOADING_TIMEOUT_TICKS: usize = 100;
    // Start the app in the MainMenu state.
    let mut app_state = AppState::MainMenu { selected: MainMenuSelection::Default };
    
    let mut cpu_by_account_data: Option<Result<UsageData, AppError>> = None;
    let mut cpu_by_node_data: Option<Result<UsageData, AppError>> = None;
    let mut gpu_by_type_data: Option<Result<UsageData, AppError>> = None;
    let mut cpu_by_account_capacity: Option<Result<CapacityData, AppError>> = None;
    let mut cpu_by_node_capacity: Option<Result<CapacityData, AppError>> = None;
    let mut gpu_by_type_capacity: Option<Result<CapacityData, AppError>> = None;

    let mut data_fetch_count = 0;

    let mut current_query_range = 7;
    let mut current_query_time_scale = PrometheusTimeScale::Days;

    loop {
        terminal.draw(|f| ui(f, &app_state))?;

        if data_fetch_count < 6 {
            if let Ok(fetched_data) = rx.try_recv() {
                data_fetch_count += 1;
                match fetched_data {
                    FetchedData::CpuByAccount(res) => cpu_by_account_data = Some(res),
                    FetchedData::CpuByNode(res) => cpu_by_node_data = Some(res),
                    FetchedData::GpuByType(res) => gpu_by_type_data = Some(res),
                    FetchedData::CpuCapacityByAccount(res) => cpu_by_account_capacity = Some(res),
                    FetchedData::CpuCapacityByNode(res) => cpu_by_node_capacity = Some(res),
                    FetchedData::GpuCapacityByType(res) => gpu_by_type_capacity = Some(res),
                }
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    if let AppState::Loaded(ref mut app) = app_state {
                        app.should_quit = true;
                    } else {
                        return Ok(());
                    }
                }

                // REFACTORED: This entire block is restructured for clarity and correctness.
                match &mut app_state {
                    AppState::MainMenu { selected } => {
                        match key.code {
                            KeyCode::Up | KeyCode::PageUp | KeyCode::Down | KeyCode::PageDown | KeyCode::Char('k') | KeyCode::Char('j')=> *selected = selected.toggle(),
                            KeyCode::Enter => {
                                match selected {
                                    MainMenuSelection::Default => {
                                        if data_fetch_count == 6 {
                                            app_state = build_loaded_app(
                                                &mut cpu_by_account_data, &mut cpu_by_node_data, &mut gpu_by_type_data,
                                                &mut cpu_by_account_capacity, &mut cpu_by_node_capacity, &mut gpu_by_type_capacity,
                                                current_query_range, current_query_time_scale
                                            );
                                        } else {
                                            app_state = AppState::Loading { tick: 0 };
                                        }
                                    },
                                    MainMenuSelection::Custom => {
                                        app_state = AppState::ParameterSelection(ParameterSelectionState::default());
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                    AppState::ParameterSelection(state) => {
                        match (key.code, state.focused_widget) {
                            // --- Global Keys for this state ---
                            (KeyCode::Tab, _) => state.focused_widget = state.focused_widget.next(),
                            
                            // local navigation keys
                            (KeyCode::Enter, ParameterFocus::Range) => state.focused_widget = state.focused_widget.next(),
                            (KeyCode::Enter, ParameterFocus::Unit) => state.focused_widget = state.focused_widget.next(),
                        
                            // --- Range Input Keys ---
                            (KeyCode::Char(c), ParameterFocus::Range) if c.is_ascii_digit() => {
                                state.range_input.push(c);
                            }
                            (KeyCode::Backspace, ParameterFocus::Range) => {
                                state.range_input.pop();
                            }

                            // --- Unit Selector Keys ---
                            (KeyCode::Left, ParameterFocus::Unit) => {
                                state.selected_unit = state.selected_unit.prev();
                            }
                            (KeyCode::Char('h'), ParameterFocus::Unit) => {
                                state.selected_unit = state.selected_unit.prev();
                            }
                            (KeyCode::Right, ParameterFocus::Unit) => {
                                state.selected_unit = state.selected_unit.next();
                            }
                            (KeyCode::Char('l'), ParameterFocus::Unit) => {
                                state.selected_unit = state.selected_unit.next();
                            }

                            // --- Confirm Button Keys ---
                            (KeyCode::Enter, ParameterFocus::Confirm) => {
                                if let Ok(range) = state.range_input.parse::<i64>() {
                                    if range > 0 {
                                        let (tx_new, rx_new) = mpsc::channel(6);
                                        rx = rx_new;
                                        cpu_by_account_data = None;
                                        cpu_by_node_data = None;
                                        gpu_by_type_data = None;
                                        cpu_by_account_capacity = None;
                                        cpu_by_node_capacity = None;
                                        gpu_by_type_capacity = None;
                                        data_fetch_count = 0;

                                        current_query_range = range;
                                        current_query_time_scale = state.selected_unit;

                                        spawn_custom_data_fetch(tx_new, range, state.selected_unit);
                                        app_state = AppState::Loading { tick: 0 };
                                    }
                                }
                            }
                            // Ignore all other key presses
                            _ => {}
                        }
                    }

                    // MODIFIED: Event handler is now a state machine based on scroll_mode.
                    AppState::Loaded(app) => {
                        match app.scroll_mode {
                            ScrollMode::Page => match key.code {
                                KeyCode::Char('1') => app.current_view = AppView::CpuByAccount,
                                KeyCode::Char('2') => app.current_view = AppView::CpuByNode,
                                KeyCode::Char('3') => app.current_view = AppView::GpuByType,
                                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => app.next_view(),
                                KeyCode::Left | KeyCode::Char('h') => app.prev_view(),
                                KeyCode::Up | KeyCode::PageUp | KeyCode::Char('k') => app.scroll_offset = app.scroll_offset.saturating_sub(1),
                                KeyCode::Down | KeyCode::PageDown | KeyCode::Char('j') => app.scroll_offset = app.scroll_offset.saturating_add(1),
                                KeyCode::Enter => app.scroll_mode = ScrollMode::Chart,
                                _ => {}
                            },
                            ScrollMode::Chart => {
                                let current_chart_data = match app.current_view {
                                    AppView::CpuByAccount => &mut app.cpu_by_account,
                                    AppView::CpuByNode => &mut app.cpu_by_node,
                                    AppView::GpuByType => &mut app.gpu_by_type,
                                };
                                match key.code {
                                    KeyCode::Right | KeyCode::Char('l') => {
                                        let max_points = current_chart_data.source_data.values()
                                            .map(|v| v.len())
                                            .max()
                                            .unwrap_or(0);
                                        
                                        let max_h_scroll = max_points.saturating_sub(MAX_BARS_PER_CHART);

                                        if current_chart_data.horizontal_scroll_offset < max_h_scroll {
                                            current_chart_data.horizontal_scroll_offset = current_chart_data
                                                .horizontal_scroll_offset.saturating_add(1);
                                        }
                                    },
                                    KeyCode::Left | KeyCode::Char('h') => {
                                        current_chart_data.horizontal_scroll_offset = current_chart_data
                                            .horizontal_scroll_offset.saturating_sub(1);
                                    },
                                    KeyCode::Esc => app.scroll_mode = ScrollMode::Page,
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {} // No input for Loading or Error states.
                }
            }
        }

        // should we be able to quit out of a loading screen to go back to the main menu?
        // would it result in any other bugs to allow this?

        if let AppState::Loading { ref mut tick } = app_state {
            *tick += 1;

            if *tick > LOADING_TIMEOUT_TICKS {
                app_state = AppState::Error(AppError::TimeOut);
                continue; // Skip the rest of the loop to immediately draw the error screen.
            }

            if data_fetch_count == 6 {
                app_state = build_loaded_app(
                    &mut cpu_by_account_data, &mut cpu_by_node_data, &mut gpu_by_type_data,
                    &mut cpu_by_account_capacity, &mut cpu_by_node_capacity, &mut gpu_by_type_capacity,
                    current_query_range, current_query_time_scale
                );
            }
        }

        if let AppState::Loaded(app) = &app_state {
            if app.should_quit {
                return Ok(());
            }
        }
    }
}



#[allow(clippy::too_many_arguments)]
fn build_loaded_app(
    cpu_by_account_data: &mut Option<Result<UsageData, AppError>>,
    cpu_by_node_data: &mut Option<Result<UsageData, AppError>>,
    gpu_by_type_data: &mut Option<Result<UsageData, AppError>>,
    cpu_by_account_capacity: &mut Option<Result<CapacityData, AppError>>,
    cpu_by_node_capacity: &mut Option<Result<CapacityData, AppError>>,
    gpu_by_type_capacity: &mut Option<Result<CapacityData, AppError>>,
    query_range: i64,
    query_time_scale: PrometheusTimeScale,
) -> AppState {
    let error_checks = [
        cpu_by_account_data.as_ref().and_then(|r| r.as_ref().err().cloned()),
        cpu_by_node_data.as_ref().and_then(|r| r.as_ref().err().cloned()),
        gpu_by_type_data.as_ref().and_then(|r| r.as_ref().err().cloned()),
        cpu_by_account_capacity.as_ref().and_then(|r| r.as_ref().err().cloned()),
        cpu_by_node_capacity.as_ref().and_then(|r| r.as_ref().err().cloned()),
        gpu_by_type_capacity.as_ref().and_then(|r| r.as_ref().err().cloned()),
    ];

    if let Some(err_opt) = error_checks.iter().flatten().next() {
        return AppState::Error(err_opt.clone());
    }

    let final_cpu_by_account = {
        let usage = cpu_by_account_data.take().unwrap().unwrap();
        let capacity = cpu_by_account_capacity.take().unwrap().unwrap();
        let max_points = usage.source_data.values().map(|v| v.len()).max().unwrap_or(0);
        let initial_offset = max_points.saturating_sub(MAX_BARS_PER_CHART);
        ChartData { source_data: usage.source_data, capacity_data: capacity.capacities, horizontal_scroll_offset: initial_offset }
    };
    let final_cpu_by_node = {
        let usage = cpu_by_node_data.take().unwrap().unwrap();
        let capacity = cpu_by_node_capacity.take().unwrap().unwrap();
        let max_points = usage.source_data.values().map(|v| v.len()).max().unwrap_or(0);
        let initial_offset = max_points.saturating_sub(MAX_BARS_PER_CHART);
        ChartData { source_data: usage.source_data, capacity_data: capacity.capacities, horizontal_scroll_offset: initial_offset }
    };
    let final_gpu_by_type = {
        let usage = gpu_by_type_data.take().unwrap().unwrap();
        let capacity = gpu_by_type_capacity.take().unwrap().unwrap();
        let max_points = usage.source_data.values().map(|v| v.len()).max().unwrap_or(0);
        let initial_offset = max_points.saturating_sub(MAX_BARS_PER_CHART);
        ChartData { source_data: usage.source_data, capacity_data: capacity.capacities, horizontal_scroll_offset: initial_offset}
    };

    let app = App {
        current_view: AppView::CpuByAccount,
        scroll_offset: 0,
        scroll_mode: ScrollMode::default(),
        cpu_by_account: final_cpu_by_account,
        cpu_by_node: final_cpu_by_node,
        gpu_by_type: final_gpu_by_type,
        should_quit: false,
        query_range,
        query_time_scale,
    };
    AppState::Loaded(app)
}

#[tokio::main]
pub async fn tui_execute() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // MODIFIED: Start fetching default data immediately.
    let (tx, rx) = mpsc::channel(6);
    spawn_custom_data_fetch(tx, 7, PrometheusTimeScale::Days);

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
