pub mod domain;
pub mod serde;
pub mod state;

pub use serde::{ReadOperationError, WriteOperationError, read_operations, write_accounts};
pub use state::Store;

use std::io::{Read, Write};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Read(#[from] ReadOperationError),
    #[error(transparent)]
    Write(#[from] WriteOperationError),
}

pub fn run<R: Read, W: Write>(reader: R, writer: W, store: &mut Store) -> Result<(), RunError> {
    for operation in read_operations(reader) {
        let operation = match operation {
            Ok(operation) => operation,
            Err(error @ ReadOperationError::Csv(_)) => return Err(error.into()),
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        };

        if let Err(error) = store.apply_operation(operation) {
            eprintln!("{error}");
        }
    }

    let mut writer = csv::Writer::from_writer(writer);
    write_accounts(store.accounts(), &mut writer)?;

    Ok(())
}
