pub mod limits;

use clap::Parser;
use fi_slurm::utils::{SlurmConfig, initialize_slurm};

use crate::limits::{leaderboard_feature, leaderboard, print_limits};



fn main() -> Result<(), String> {

    let args = Args::parse();

    initialize_slurm();
    let _slurm_config = SlurmConfig::load()?;
    // not clear we need to load config, but let's test that later

    match args.leaderboard {
        None => {}, // do nothing
        Some(num) => { // number is imputed from default of 20
            if args.filter.is_empty() {
                leaderboard(num);
                return Ok(())
            } else {
                println!("\nFiltering on: {:?}", args.filter);
                leaderboard_feature(num, args.filter);
                return Ok(())
            }
        }
    }

    let qos_name = if !args.user.is_empty() {
        args.user.first()
    } else {
        None
    };

    // make this the default if the leaderboard is not in use
    print_limits(qos_name);
    Ok(())

}


#[derive(Parser, Debug)]
#[command(version, about, long_about = "This command-line and terminal application was built by Lehman Garrison, Nicolas Posner, Dylan Simon, and Alex Chavkin at the Scientific Computing Core of the Flatiron Institute. By default, it displays the current resource usage and limits of the user and their center.")]
struct Args {
    #[arg(short, long)]
    user: Vec<String>,
    #[arg(short, long)]
    #[arg(num_args(0..))]
    #[arg(help = "Select individual features to filter by. `icelake` would only show information for icelake nodes. \n For multiple features, separate them with spaces, such as `genoa gpu skylake`")]
    filter: Vec<String>,
    #[arg(long)]
    #[arg(num_args(0..=1))]
    #[arg(default_missing_value = "20")]
    #[arg(help = "Display a leaderboard of current cluster use by user, according to node and core use. If no number is passed, it defaults to showing the top 20")]
    leaderboard: Option<usize>,
}



