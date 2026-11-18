mod state;

use serde::Deserialize;
use std::{fs::File, io, path::PathBuf, println};

use clap::{Arg, Command, value_parser};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClientId(u16);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TxId(u32);

#[derive(Debug)]
enum DisputeKind {
    Open,
    Resolve,
    ChargeBack,
}

#[derive(Debug)]
struct Dispute {
    kind: DisputeKind,
    client: ClientId,
    tx: TxId,
}

#[derive(Debug)]
enum TransactionKind {
    Deposit,
    Withdrawal,
}

#[derive(Debug)]
struct Transaction {
    kind: TransactionKind,
    client: ClientId,
    tx: TxId,
    amount: Decimal,
}

#[derive(Debug)]
enum Operation {
    Transaction(Transaction),
    Dispute(Dispute),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OperationKind {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    ChargeBack,
}

#[derive(Debug, Deserialize)]
struct OperationRow {
    #[serde(rename = "type")]
    kind: OperationKind,
    client: u16,
    tx: u32,
    // TODO: would love to make amount an unsigned decimal
    amount: Option<Decimal>,
}

impl TryFrom<OperationRow> for Operation {
    type Error = &'static str;

    fn try_from(row: OperationRow) -> Result<Self, Self::Error> {
        match row.kind {
            OperationKind::Deposit => {
                let amount = row.amount.ok_or("Deposit transaction is missing amount.")?;

                Ok(Self::Transaction(Transaction {
                    kind: TransactionKind::Deposit,
                    client: ClientId(row.client),
                    tx: TxId(row.tx),
                    amount,
                }))
            }
            OperationKind::Withdrawal => {
                let amount = row
                    .amount
                    .ok_or("Withdrawal transaction is missing amount.")?;

                Ok(Self::Transaction(Transaction {
                    kind: TransactionKind::Withdrawal,
                    client: ClientId(row.client),
                    tx: TxId(row.tx),
                    amount,
                }))
            }
            OperationKind::Dispute => Ok(Self::Dispute(Dispute {
                kind: DisputeKind::Open,
                client: ClientId(row.client),
                tx: TxId(row.tx),
            })),
            OperationKind::Resolve => Ok(Self::Dispute(Dispute {
                kind: DisputeKind::Resolve,
                client: ClientId(row.client),
                tx: TxId(row.tx),
            })),
            OperationKind::ChargeBack => Ok(Self::Dispute(Dispute {
                kind: DisputeKind::ChargeBack,
                client: ClientId(row.client),
                tx: TxId(row.tx),
            })),
        }
    }
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

    let reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;

    for row in reader.into_deserialize::<OperationRow>() {
        let op: Result<Operation, &'static str> = row?.try_into();
        println!("{:?}", op);
    }

    let _output = io::stdout().lock();

    // TODO: Process transactions from _input and write account CSV to _output.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ClientId, Dispute, DisputeKind, Operation, OperationKind, OperationRow, Transaction,
        TransactionKind, TxId,
    };
    use rust_decimal::Decimal;

    fn row(kind: OperationKind, amount: Option<Decimal>) -> OperationRow {
        OperationRow {
            kind,
            client: 42,
            tx: 123,
            amount,
        }
    }

    #[test]
    fn deposit_with_amount_converts() {
        let amount = Decimal::new(123456, 4);
        let operation = Operation::try_from(row(OperationKind::Deposit, Some(amount)))
            .expect("deposit with an amount should convert");

        assert!(matches!(
            operation,
            Operation::Transaction(Transaction {
                kind: TransactionKind::Deposit,
                client: ClientId(42),
                tx: TxId(123),
                amount: actual,
            }) if actual == amount
        ));
    }

    #[test]
    fn withdrawal_with_amount_converts() {
        let amount = Decimal::new(654321, 4);
        let operation = Operation::try_from(row(OperationKind::Withdrawal, Some(amount)))
            .expect("withdrawal with an amount should convert");

        assert!(matches!(
            operation,
            Operation::Transaction(Transaction {
                kind: TransactionKind::Withdrawal,
                client: ClientId(42),
                tx: TxId(123),
                amount: actual,
            }) if actual == amount
        ));
    }

    #[test]
    fn dispute_without_amount_converts() {
        let operation = Operation::try_from(row(OperationKind::Dispute, None))
            .expect("dispute without an amount should convert");

        assert!(matches!(
            operation,
            Operation::Dispute(Dispute {
                kind: DisputeKind::Open,
                client: ClientId(42),
                tx: TxId(123),
            })
        ));
    }

    #[test]
    fn resolve_without_amount_converts() {
        let operation = Operation::try_from(row(OperationKind::Resolve, None))
            .expect("resolve without an amount should convert");

        assert!(matches!(
            operation,
            Operation::Dispute(Dispute {
                kind: DisputeKind::Resolve,
                client: ClientId(42),
                tx: TxId(123),
            })
        ));
    }

    #[test]
    fn chargeback_without_amount_converts() {
        let operation = Operation::try_from(row(OperationKind::ChargeBack, None))
            .expect("chargeback without an amount should convert");

        assert!(matches!(
            operation,
            Operation::Dispute(Dispute {
                kind: DisputeKind::ChargeBack,
                client: ClientId(42),
                tx: TxId(123),
            })
        ));
    }

    #[test]
    fn deposit_without_amount_returns_error() {
        let error = Operation::try_from(row(OperationKind::Deposit, None))
            .expect_err("deposit requires an amount");

        assert_eq!(error, "Deposit transaction is missing amount.");
    }

    #[test]
    fn withdrawal_without_amount_returns_error() {
        let error = Operation::try_from(row(OperationKind::Withdrawal, None))
            .expect_err("withdrawal requires an amount");

        assert_eq!(error, "Withdrawal transaction is missing amount.");
    }

    #[test]
    fn dispute_ignores_supplied_amount() {
        let operation =
            Operation::try_from(row(OperationKind::Dispute, Some(Decimal::new(123456, 4))))
                .expect("dispute should ignore a supplied amount");

        assert!(matches!(
            operation,
            Operation::Dispute(Dispute {
                kind: DisputeKind::Open,
                client: ClientId(42),
                tx: TxId(123),
            })
        ));
    }

    #[test]
    fn resolve_ignores_supplied_amount() {
        let operation =
            Operation::try_from(row(OperationKind::Resolve, Some(Decimal::new(123456, 4))))
                .expect("resolve should ignore a supplied amount");

        assert!(matches!(
            operation,
            Operation::Dispute(Dispute {
                kind: DisputeKind::Resolve,
                client: ClientId(42),
                tx: TxId(123),
            })
        ));
    }

    #[test]
    fn chargeback_ignores_supplied_amount() {
        let operation = Operation::try_from(row(
            OperationKind::ChargeBack,
            Some(Decimal::new(123456, 4)),
        ))
        .expect("chargeback should ignore a supplied amount");

        assert!(matches!(
            operation,
            Operation::Dispute(Dispute {
                kind: DisputeKind::ChargeBack,
                client: ClientId(42),
                tx: TxId(123),
            })
        ));
    }
}
