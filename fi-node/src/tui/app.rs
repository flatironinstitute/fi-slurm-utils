use crate::tui::{
    interface::{
        PrometheusTimeScale,
        get_cpu_by_account_data_async,
        get_cpu_by_node_data_async,
        get_gpu_by_type_data_async,
        get_cpu_capacity_by_account_async,
        get_cpu_capacity_by_node_async,
        get_gpu_capacity_by_type_async,
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

// MODIFIED: This struct no longer holds a single max_capacity.
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
        self.scroll_offset = 0
    }

    fn prev_view(&mut self) {
        self.current_view = match self.current_view {
            AppView::CpuByAccount => AppView::GpuByType,
            AppView::CpuByNode => AppView::CpuByAccount,
            AppView::GpuByType => AppView::CpuByNode,
        };
        self.scroll_offset = 0
    }
}

#[allow(clippy::large_enum_variant)]
pub enum AppState {
    Loading { tick: usize },
    Loaded(App),
    Error(AppError),
}

#[derive(Debug)]
pub struct UsageData {
    pub source_data: HashMap<String, Vec<u64>>,
}

// MODIFIED: This struct no longer holds a single max_capacity.
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




async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut rx: mpsc::Receiver<FetchedData>,
) -> io::Result<()> {
    let mut app_state = AppState::Loading { tick: 0 };

    let mut cpu_by_account_data: Option<Result<UsageData, AppError>> = None;
    let mut cpu_by_node_data: Option<Result<UsageData, AppError>> = None;
    let mut gpu_by_type_data: Option<Result<UsageData, AppError>> = None;
    let mut cpu_by_account_capacity: Option<Result<CapacityData, AppError>> = None;
    let mut cpu_by_node_capacity: Option<Result<CapacityData, AppError>> = None;
    let mut gpu_by_type_capacity: Option<Result<CapacityData, AppError>> = None;

    let mut data_fetch_count = 0;

    loop {
        terminal.draw(|f| ui(f, &app_state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    if let AppState::Loaded(ref mut app) = app_state {
                        app.should_quit = true;
                    } else {
                        return Ok(());
                    }
                }
                
                if let AppState::Loaded(ref mut app) = app_state {
                    match key.code {
                        KeyCode::Char('1') => app.current_view = AppView::CpuByAccount,
                        KeyCode::Char('2') => app.current_view = AppView::CpuByNode,
                        KeyCode::Char('3') => app.current_view = AppView::GpuByType,
                        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => app.next_view(),
                        KeyCode::Left | KeyCode::Char('h') => app.prev_view(),
                        KeyCode::Up => app.scroll_offset = app.scroll_offset.saturating_sub(1),
                        KeyCode::Down => app.scroll_offset = app.scroll_offset.saturating_add(1),
                        _ => {}
                    }
                }
            }
        }

        if let AppState::Loading {..} = app_state {
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
        
        if let AppState::Loading { ref mut tick } = app_state {
            *tick += 1;

            if data_fetch_count == 6 {
                let mut first_error: Option<AppError> = None;
                
                let error_checks = [
                    cpu_by_account_data.as_ref().and_then(|r| r.as_ref().err().cloned()),
                    cpu_by_node_data.as_ref().and_then(|r| r.as_ref().err().cloned()),
                    gpu_by_type_data.as_ref().and_then(|r| r.as_ref().err().cloned()),
                    cpu_by_account_capacity.as_ref().and_then(|r| r.as_ref().err().cloned()),
                    cpu_by_node_capacity.as_ref().and_then(|r| r.as_ref().err().cloned()),
                    gpu_by_type_capacity.as_ref().and_then(|r| r.as_ref().err().cloned()),
                ];

                if let Some(err_opt) = error_checks.iter().flatten().next() {
                    first_error = Some(err_opt.clone());
                }

                if let Some(error) = first_error {
                    app_state = AppState::Error(error);
                } else {
                    // Combine usage and capacity data into the final ChartData structs.
                    let final_cpu_by_account = {
                        let usage = cpu_by_account_data.take().unwrap().unwrap();
                        let capacity = cpu_by_account_capacity.take().unwrap().unwrap();
                        ChartData {
                            source_data: usage.source_data,
                            capacity_data: capacity.capacities,
                        }
                    };

                    let final_cpu_by_node = {
                        let usage = cpu_by_node_data.take().unwrap().unwrap();
                        let capacity = cpu_by_node_capacity.take().unwrap().unwrap();
                        ChartData {
                            source_data: usage.source_data,
                            capacity_data: capacity.capacities,
                        }
                    };

                    let final_gpu_by_type = {
                        let usage = gpu_by_type_data.take().unwrap().unwrap();
                        let capacity = gpu_by_type_capacity.take().unwrap().unwrap();
                        ChartData {
                            source_data: usage.source_data,
                            capacity_data: capacity.capacities,
                        }
                    };

                    let app = App {
                        current_view: AppView::CpuByAccount,
                        scroll_offset: 0,
                        cpu_by_account: final_cpu_by_account,
                        cpu_by_node: final_cpu_by_node,
                        gpu_by_type: final_gpu_by_type,
                        should_quit: false,
                    };
                    app_state = AppState::Loaded(app);
                }
            }
        }

        if let AppState::Loaded(app) = &app_state {
            if app.should_quit {
                return Ok(());
            }
        }
    }
}

#[tokio::main]
pub async fn tui_execute() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel(6);

    tokio::spawn(get_cpu_by_account_data_async(tx.clone(), 7, PrometheusTimeScale::Day));
    tokio::spawn(get_cpu_by_node_data_async(tx.clone(), 7, PrometheusTimeScale::Day));
    tokio::spawn(get_gpu_by_type_data_async(tx.clone(), 7, PrometheusTimeScale::Day));
    tokio::spawn(get_cpu_capacity_by_account_async(tx.clone(), 7, PrometheusTimeScale::Day));
    tokio::spawn(get_cpu_capacity_by_node_async(tx.clone(), 7, PrometheusTimeScale::Day));
    tokio::spawn(get_gpu_capacity_by_type_async(tx.clone(), 7, PrometheusTimeScale::Day));

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


