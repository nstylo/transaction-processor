mod domain;
mod serde;
mod state;

use clap::{Arg, Command, value_parser};
use std::{io, path::PathBuf, println};

use crate::{
    domain::Operation,
    serde::{AccountRow, OperationRow},
    state::Store,
};

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

    let reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;

    let mut store = Store::new();

    for row in reader.into_deserialize::<OperationRow>() {
        let op: Result<Operation, &'static str> = row?.try_into();
        println!("{:?}", op);
        match op {
            Ok(op) => match store.apply_operation(op) {
                Ok(()) => {}
                Err(err) => {
                    println!("{:?}", err);
                }
            },
            Err(err) => {
                println!("{:?}", err);
            }
        }
    }

    for account in store.accounts() {
        let row: AccountRow = account.into();
        println!("{:?}", row);
    }

    let _output = io::stdout().lock();

    Ok(())
}
