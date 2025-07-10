use crate::tui::{
    interface::*,
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
}

pub struct App {
    pub current_view: AppView,
    pub cpu_by_account: ChartData,
    pub cpu_by_node: ChartData,
    pub gpu_by_type: ChartData,
    pub should_quit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    CpuByAccount,
    CpuByNode,
    GpuByType,
}

#[allow(clippy::large_enum_variant)]
pub enum AppState {
    Loading { tick: usize },
    Loaded(App),
    Error(AppError),
}

#[derive(Debug)]
pub enum FetchedData {
    CpuByAccount(Result<ChartData, AppError>),
    CpuByNode(Result<ChartData, AppError>),
    GpuByType(Result<ChartData, AppError>),
    CpuCapacityByAccount(Result<ChartCapacity, AppError>),
    CpuCapacityByNode(Result<ChartCapacity, AppError>),
    GpuCapacityByType(Result<ChartCapacity, AppError>),
}

//#[derive(Debug)]
//pub enum FetchedCapacity {
//    CpuByAccount(Result<ChartCapacity, AppError>),
//    CpuByNode(Result<ChartCapacity, AppError>),
//    GpuByType(Result<ChartCapacity, AppError>),
//}

#[derive(Debug)]
pub struct ChartData {
    pub source_data: HashMap<String, Vec<u64>>,
}

#[derive(Debug)]
pub struct ChartCapacity {
    pub capacity_vec: HashMap<String, u64>,
    pub max_capacity: u64,
}

#[tokio::main]
pub async fn tui_execute() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel(6);

    tokio::spawn(get_cpu_by_account_data_async(tx.clone()));
    tokio::spawn(get_cpu_by_node_data_async(tx.clone()));
    tokio::spawn(get_gpu_by_type_data_async(tx.clone()));

    tokio::spawn(get_cpu_capacity_by_account_async(tx.clone()));
    tokio::spawn(get_cpu_capacity_by_node_async(tx.clone()));
    tokio::spawn(get_gpu_capacity_by_type_async(tx.clone()));

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
    mut rx: mpsc::Receiver<FetchedData>,
) -> io::Result<()> {
    let mut app_state = AppState::Loading { tick: 0 };

    // ERROR HANDLING: We now store Results, not just the data.
    let mut cpu_by_account_data: Option<Result<ChartData, AppError>> = None;
    let mut cpu_by_node_data: Option<Result<ChartData, AppError>> = None;
    let mut gpu_by_type_data: Option<Result<ChartData, AppError>> = None;
    let mut cpu_by_account_capacity: Option<Result<ChartCapacity, AppError>> = None;
    let mut cpu_by_node_capacity: Option<Result<ChartCapacity, AppError>> = None;
    let mut gpu_by_type_capacity: Option<Result<ChartCapacity, AppError>> = None;

    let mut data_fetch_count = 0;

    loop {
        terminal.draw(|f| ui(f, &app_state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Quit works in any state
                if key.code == KeyCode::Char('q') {
                     if let AppState::Loaded(ref mut app) = app_state {
                        app.should_quit = true;
                    } else {
                        // Quit immediately if loading or in error state
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
                        _ => {}
                    }
                }
            }
        }

        // Only process messages if we are in the Loading state
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
            // Check if all tasks have reported back
            if data_fetch_count == 3 {

                let mut first_error: Option<AppError> = None;
                for res_opt in [&cpu_by_account_data, &cpu_by_node_data, &gpu_by_type_data].into_iter().flatten() {
                    if let Err(err) = res_opt {
                        first_error = Some(err.clone());
                        break; // Found an error, no need to check further
                    }
                }

                // 2. Transition state based on whether an error was found.
                if let Some(error) = first_error {
                    app_state = AppState::Error(error);
                } else {
                    // All data is loaded successfully, create the App.
                    // The .unwrap().unwrap() is safe here because we know all results are Some(Ok(...))
                    let app = App {
                        current_view: AppView::CpuByAccount,
                        cpu_by_account: cpu_by_account_data.take().unwrap().unwrap(),
                        cpu_by_node: cpu_by_node_data.take().unwrap().unwrap(),
                        gpu_by_type: gpu_by_type_data.take().unwrap().unwrap(),
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

// --- App State and Data Loading ---

impl App {
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

