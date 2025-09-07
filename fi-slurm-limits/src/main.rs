pub mod limits;

use clap::Parser;
use fi_slurm::utils::{SlurmConfig, initialize_slurm};

use crate::limits::{leaderboard, leaderboard_feature, print_limits};

use users::get_current_username;

/// The main function for the fi-slurm-limits CLI application
/// Parses the inputs and manages the pipeline for the fi-slurm-limits and leaderboard utilities
fn main() -> Result<(), String> {
    let args = Args::parse();

    initialize_slurm();
    let _slurm_config = SlurmConfig::load()?;
    // not clear we need to load config, but let's test that later

    match args.leaderboard {
        None => {} // do nothing
        Some(num) => {
            // number is imputed from default of 20
            if args.filter.is_empty() {
                leaderboard(num);
                return Ok(());
            } else {
                println!("\nFiltering on: {:?}", args.filter);
                leaderboard_feature(num, args.filter);
                return Ok(());
            }
        }
    }

    // getting the user name passed in, if it exists, or else passes in None,
    // which will cause the print_limits function to get the username from OS
    let user_name = args.user.unwrap_or_else(|| {
        get_current_username()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    });

    print_limits(&user_name);
    Ok(())
}

const HELP: &str = "it displays the current resource usage and limits of the user and their center";

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    after_help = HELP,
    after_long_help = format!("{}\n\n{}", HELP, fi_slurm::AUTHOR_HELP),
)]
struct Args {
    #[arg(help = "The username for which to show limits. Defaults to the current user.")]
    user: Option<String>,

    #[arg(short, long)]
    #[arg(num_args(0..=1))]
    #[arg(value_name = "TOP_N")]
    #[arg(default_missing_value = "10")]
    #[arg(help = "Display the users with the highest current cluster usage. Defaults to top 10.")]
    leaderboard: Option<usize>,

    #[arg(short, long)]
    #[arg(num_args(0..))]
    #[arg(
        help = "For use with the leaderboard: select individual features to filter by. `icelake` would only show information for icelake nodes. \n For multiple features, separate them with spaces, such as `genoa gpu skylake`"
    )]
    filter: Vec<String>,
}
