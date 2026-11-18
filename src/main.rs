use clap::{Arg, Command, value_parser};
use std::{fs::File, io, path::PathBuf};
use transaction_processor::{Store, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("Transaction")
        .arg(
            Arg::new("transactions_path")
                .value_name("TRANSACTIONS_PATH")
                .help("Path to the input transactions file. File expected to be formatted as a valid csv file.")
                .required(true)
                .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let path = matches
        .get_one::<PathBuf>("transactions_path")
        .expect("transactions is a required argument");

    let input = File::open(path)?;
    let mut store = Store::new();

    run(input, io::stdout().lock(), &mut store)?;

    Ok(())
}
