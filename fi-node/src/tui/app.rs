use crate::tui::{
    interface::{
        get_cpu_by_account_data_async, get_cpu_by_node_data_async,
        get_gpu_by_type_data_async, get_cpu_capacity_by_account_async,
        get_cpu_capacity_by_node_async, get_gpu_capacity_by_type_async,
        PrometheusTimeScale,
    },
    ui::ui,
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    CpuByAccount,
    CpuByNode,
    GpuByType,
}

#[derive(Debug)]
pub struct ChartData {
    pub source_data: HashMap<String, Vec<u64>>,
    pub capacity_data: HashMap<String, Vec<u64>>,
}

pub struct App {
    pub current_view: AppView,
    pub scroll_offset: usize,
    pub cpu_by_account: ChartData,
    pub cpu_by_node: ChartData,
    pub gpu_by_type: ChartData,
    pub should_quit: bool,
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

#[derive(Debug, Default, Clone)]
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

#[derive(Debug, Clone)]
pub struct UsageData {
    pub source_data: HashMap<String, Vec<u64>>,
}

#[derive(Debug, Clone)]
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
    // Start the app in the MainMenu state.
    let mut app_state = AppState::MainMenu { selected: MainMenuSelection::Default };
    
    let mut cpu_by_account_data: Option<Result<UsageData, AppError>> = None;
    let mut cpu_by_node_data: Option<Result<UsageData, AppError>> = None;
    let mut gpu_by_type_data: Option<Result<UsageData, AppError>> = None;
    let mut cpu_by_account_capacity: Option<Result<CapacityData, AppError>> = None;
    let mut cpu_by_node_capacity: Option<Result<CapacityData, AppError>> = None;
    let mut gpu_by_type_capacity: Option<Result<CapacityData, AppError>> = None;

    let mut data_fetch_count = 0;

    loop {
        // DEBUG: Print the state that is about to be rendered.
        eprintln!("\n--- TOP OF LOOP: DRAWING FRAME ---");
        //eprintln!("Current App State: {:?}", app_state);
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
                eprintln!("\n--- EVENT DETECTED ---");
                eprintln!("Key pressed: {:?}", key.code);

                if key.code == KeyCode::Char('q') {
                    if let AppState::Loaded(ref mut app) = app_state {
                        app.should_quit = true;
                    } else {
                        eprintln!("Quitting from non-loaded state.");
                        return Ok(());
                    }
                }

                match &mut app_state {
                    AppState::MainMenu { selected } => {
                        eprintln!("Handling event in MainMenu state.");
                        match key.code {
                            KeyCode::Up | KeyCode::Down | KeyCode::Char('k') | KeyCode::Char('j')=> *selected = selected.toggle(),
                            KeyCode::Enter => {
                                match selected {
                                    MainMenuSelection::Default => {
                                        if data_fetch_count == 6 {
                                            app_state = build_loaded_app(
                                                &mut cpu_by_account_data, &mut cpu_by_node_data, &mut gpu_by_type_data,
                                                &mut cpu_by_account_capacity, &mut cpu_by_node_capacity, &mut gpu_by_type_capacity
                                            );
                                        } else {
                                            app_state = AppState::Loading { tick: 0 };
                                        }
                                    },
                                    MainMenuSelection::Custom => {
                                        eprintln!("Transitioning to ParameterSelection state.");
                                        app_state = AppState::ParameterSelection(ParameterSelectionState::default());
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                    AppState::ParameterSelection(state) => {
                        eprintln!("Handling event in ParameterSelection state. Focus is on: {:?}", state.focused_widget);
                        match (key.code, state.focused_widget) {
                            (KeyCode::Tab, _) => {
                                state.focused_widget = state.focused_widget.next();
                                eprintln!("Focus changed to: {:?}", state.focused_widget);
                            }
                            (KeyCode::Char(c), ParameterFocus::Range) if c.is_ascii_digit() => {
                                state.range_input.push(c);
                                eprintln!("Range input updated: {}", state.range_input);
                            }
                            (KeyCode::Backspace, ParameterFocus::Range) => {
                                state.range_input.pop();
                                eprintln!("Range input after backspace: {}", state.range_input);
                            }
                            (KeyCode::Left, ParameterFocus::Unit) => {
                                state.selected_unit = state.selected_unit.prev();
                                eprintln!("Unit changed to: {:?}", state.selected_unit);
                            }
                            (KeyCode::Right, ParameterFocus::Unit) => {
                                state.selected_unit = state.selected_unit.next();
                                eprintln!("Unit changed to: {:?}", state.selected_unit);
                            }
                            (KeyCode::Enter, ParameterFocus::Confirm) => {
                                eprintln!("Enter pressed on Confirm.");
                                if let Ok(range) = state.range_input.parse::<i64>() {
                                    if range > 0 {
                                        eprintln!("Spawning new data fetch for range {} {}.", range, state.selected_unit);
                                        let (tx_new, rx_new) = mpsc::channel(6);
                                        rx = rx_new;
                                        cpu_by_account_data = None;
                                        cpu_by_node_data = None;
                                        gpu_by_type_data = None;
                                        cpu_by_account_capacity = None;
                                        cpu_by_node_capacity = None;
                                        gpu_by_type_capacity = None;
                                        data_fetch_count = 0;

                                        spawn_custom_data_fetch(tx_new, range, state.selected_unit);
                                        app_state = AppState::Loading { tick: 0 };
                                    }
                                }
                            }
                            _ => {
                                eprintln!("Key press had no effect in this context.");
                            }
                        }
                    }
                    AppState::Loaded(app) => {
                        eprintln!("Handling event in Loaded state.");
                        match key.code {
                            KeyCode::Char('1') => app.current_view = AppView::CpuByAccount,
                            KeyCode::Char('2') => app.current_view = AppView::CpuByNode,
                            KeyCode::Char('3') => app.current_view = AppView::GpuByType,
                            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => app.next_view(),
                            KeyCode::Left | KeyCode::Char('h') => app.prev_view(),
                            KeyCode::Up | KeyCode::Char('k') => app.scroll_offset = app.scroll_offset.saturating_sub(1),
                            KeyCode::Down | KeyCode::Char('j') => app.scroll_offset = app.scroll_offset.saturating_add(1),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        if let AppState::Loading { ref mut tick } = app_state {
            *tick += 1;
            if data_fetch_count == 6 {
                app_state = build_loaded_app(
                    &mut cpu_by_account_data, &mut cpu_by_node_data, &mut gpu_by_type_data,
                    &mut cpu_by_account_capacity, &mut cpu_by_node_capacity, &mut gpu_by_type_capacity
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

fn build_loaded_app(
    cpu_by_account_data: &mut Option<Result<UsageData, AppError>>,
    cpu_by_node_data: &mut Option<Result<UsageData, AppError>>,
    gpu_by_type_data: &mut Option<Result<UsageData, AppError>>,
    cpu_by_account_capacity: &mut Option<Result<CapacityData, AppError>>,
    cpu_by_node_capacity: &mut Option<Result<CapacityData, AppError>>,
    gpu_by_type_capacity: &mut Option<Result<CapacityData, AppError>>,
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
        ChartData { source_data: usage.source_data, capacity_data: capacity.capacities }
    };
    let final_cpu_by_node = {
        let usage = cpu_by_node_data.take().unwrap().unwrap();
        let capacity = cpu_by_node_capacity.take().unwrap().unwrap();
        ChartData { source_data: usage.source_data, capacity_data: capacity.capacities }
    };
    let final_gpu_by_type = {
        let usage = gpu_by_type_data.take().unwrap().unwrap();
        let capacity = gpu_by_type_capacity.take().unwrap().unwrap();
        ChartData { source_data: usage.source_data, capacity_data: capacity.capacities }
    };

    let app = App {
        current_view: AppView::CpuByAccount,
        scroll_offset: 0,
        cpu_by_account: final_cpu_by_account,
        cpu_by_node: final_cpu_by_node,
        gpu_by_type: final_gpu_by_type,
        should_quit: false,
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

    let (tx, rx) = mpsc::channel(6);
    spawn_custom_data_fetch(tx, 7, PrometheusTimeScale::Day);

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
