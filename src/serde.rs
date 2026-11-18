use crate::domain::{
    Account, ClientId, DECIMAL_PRECISION, Dispute, DisputeKind, Operation, Transaction,
    TransactionKind, TxId,
};
use rust_decimal::Decimal;
use serde::{self, Deserialize, Serialize};
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReadOperationError {
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error("{0}")]
    InvalidOperation(&'static str),
}

#[derive(Debug, Error)]
pub enum WriteOperationError {
    #[error("failed to serialize account CSV: {0}")]
    Serialize(#[from] csv::Error),
    #[error("failed to flush account CSV: {0}")]
    Flush(#[from] io::Error),
}

/// streams csv file line by line instead of reading the whole file
/// into memory.
pub fn read_operations<R: io::Read>(
    reader: R,
) -> impl Iterator<Item = Result<Operation, ReadOperationError>> {
    csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader)
        .into_deserialize::<OperationRow>()
        .map(|row| {
            let row = row?;
            row.try_into().map_err(ReadOperationError::InvalidOperation)
        })
}

pub fn write_accounts<W: io::Write>(
    accounts: impl IntoIterator<Item = Account>,
    writer: &mut csv::Writer<W>,
) -> Result<(), WriteOperationError> {
    for account in accounts {
        // handles writing header
        writer.serialize(AccountRow::from(account))?;
    }

    Ok(writer.flush()?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationKind {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    ChargeBack,
}

#[derive(Debug, Deserialize)]
pub struct OperationRow {
    // 'type' is a reserved keyword
    #[serde(rename = "type")]
    kind: OperationKind,
    client: u16,
    tx: u32,
    // TODO: would love to make amount an unsigned decimal
    #[serde(default, deserialize_with = "decimal_truncate::deserialize_optional")]
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AccountRow {
    pub client: u16,
    #[serde(with = "decimal_truncate")]
    pub available: Decimal,
    #[serde(with = "decimal_truncate")]
    pub held: Decimal,
    #[serde(with = "decimal_truncate")]
    pub total: Decimal,
    pub locked: bool,
}

mod decimal_truncate {
    use super::{DECIMAL_PRECISION, Decimal};
    use ::serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse(&text)
    }

    pub fn deserialize_optional<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Decimal>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| parse(&text))
            .transpose()
    }

    pub fn serialize<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&value.trunc_with_scale(DECIMAL_PRECISION))
    }

    fn parse<E: Error>(text: &str) -> Result<Decimal, E> {
        Decimal::from_str_exact(text)
            .map(|decimal| decimal.trunc_with_scale(DECIMAL_PRECISION))
            .map_err(E::custom)
    }
}

impl From<Account> for AccountRow {
    fn from(account: Account) -> Self {
        Self {
            client: account.client.0,
            available: account.available,
            held: account.held,
            total: account.total,
            locked: account.locked,
        }
    }
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
    fn decimal_text_is_parsed_exactly_then_truncated() {
        for (input, expected) in [
            ("9007199254740993.12349", "9007199254740993.1234"),
            ("1.99999999999999999", "1.9999"),
            ("-1.99999999999999999", "-1.9999"),
            ("+0001.23459", "1.2345"),
            (".123456", "0.1234"),
            ("1.", "1"),
            ("-0.00009", "0"),
            (
                "79228162514264337593543950335",
                "79228162514264337593543950335",
            ),
        ] {
            let result: Result<Decimal, ::serde::de::value::Error> =
                super::decimal_truncate::deserialize(::serde::de::value::StrDeserializer::new(
                    input,
                ));
            assert_eq!(
                result.unwrap(),
                Decimal::from_str_exact(expected).unwrap(),
                "{input}"
            );
        }
    }

    #[test]
    fn decimal_text_rejects_invalid_suffixes_and_unrepresentable_values() {
        for input in [
            "1.2345garbage",
            "1.2345.6",
            "1.2345-6",
            "1.2345９",
            "+-1.2345",
            "",
            ".",
            "+",
            "-",
            "1.999999999999999999999999999999999999",
            "-1.999999999999999999999999999999999999",
            "79228162514264337593543950336",
            "79228162514264337593543950335.0001",
            "79228162514264337593543950335.00009",
        ] {
            let result: Result<Decimal, ::serde::de::value::Error> =
                super::decimal_truncate::deserialize(::serde::de::value::StrDeserializer::new(
                    input,
                ));
            assert!(result.is_err(), "invalid input was accepted: {input}");
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
