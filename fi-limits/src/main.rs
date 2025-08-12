pub mod limits;

use clap::Parser;
use fi_slurm::utils::{SlurmConfig, initialize_slurm};

use crate::limits::{leaderboard_feature, leaderboard, print_limits};



fn main() {

    let args = Args::parse();

    initialize_slurm();
    let _slurm_config = SlurmConfig::load().unwrap();
    // not clear we need to load config, but let's test that later

    match args.leaderboard {
        None => {}, // do nothing
        Some(num) => { // number is imputed from default of 20
            // let top_n = args.leaderboard.unwrap_or(&20);
            if args.feature.is_empty() {
                leaderboard(num);
            } else {
                println!("{:?}", args.feature);
                leaderboard_feature(num, args.feature);
            }
        }
    }

    let qos_name = if !args.user.is_empty() {
        args.user.first()
    } else {
        None
    };

    if args.limits {
        print_limits(qos_name);
    }

}


#[derive(Parser, Debug)]
#[command(version, about, long_about = "This command-line and terminal application was built by Lehman Garrison, Nicolas Posner, Dylan Simon, and Alex Chavkin at the Scientific Computing Core of the Flatiron Institute. By default, it displays the availability of different node types as a tree diagram, as queried from the locally running instance of Slurm. For support, contact nposner@flatironinstitute.org")]
struct Args {
    #[arg(long)]
    limits: bool,
    #[arg(short, long)]
    user: Vec<String>,
    #[arg(help = "Select individual features to filter by. `icelake` would only show information for icelake nodes. \n For multiple features, separate them with spaces, such as `genoa gpu skylake`")]
    feature: Vec<String>,
    #[arg(long)]
    #[arg(num_args(0..=1))]
    #[arg(default_missing_value = "20")]
    leaderboard: Option<usize>,
}



