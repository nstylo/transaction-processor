mod domain;
mod serde;
mod state;

use clap::{Arg, Command, value_parser};
use std::{
    io::{self, Write},
    path::PathBuf,
    println,
};

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
        match op {
            Ok(op) => match store.apply_operation(op) {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("{:?}", err);
                }
            },
            Err(err) => {
                eprintln!("{:?}", err);
            }
        }
    }

    let mut output = io::stdout().lock();
    let buffer = accounts_csv(&store).unwrap();
    output.write_all(&buffer)?;

    Ok(())
}

fn accounts_csv(store: &Store) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = csv::WriterBuilder::new().from_writer(vec![]);

    for account in store.accounts() {
        writer.serialize(AccountRow::from(account))?;
    }

    Ok(writer.into_inner()?)
}
