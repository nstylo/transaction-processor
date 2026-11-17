use std::{fs::File, io, path::PathBuf, println};

use clap::{Arg, Command, value_parser};
use rust_decimal::Decimal;

struct Account {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

enum Transaction {
    Deposit {
        client: u16,
        tx: u32,
        amount: Decimal,
    },
    Withdrawal {
        client: u16,
        tx: u32,
        amount: Decimal,
    },
    Dispute {
        client: u16,
        tx: u32,
    },
    Resolve {
        client: u16,
        tx: u32,
    },
}

fn main() -> io::Result<()> {
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

    let csv_reader = csv::Reader::from_path(path)?;
    for line in csv_reader.into_records() {
        let line = line?;
        println!("{:?}", line);
    }

    let _output = io::stdout().lock();

    // TODO: Process transactions from _input and write account CSV to _output.

    Ok(())
}
