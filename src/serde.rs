use crate::domain::{
    Account, ClientId, DECIMAL_PRECISION, Dispute, DisputeKind, Operation, Transaction,
    TransactionKind, TxId,
};
use rust_decimal::Decimal;
use serde::{self, Deserialize, Serialize};

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
    amount: Option<Decimal>,
}

impl TryFrom<OperationRow> for Operation {
    type Error = &'static str;

    fn try_from(row: OperationRow) -> Result<Self, Self::Error> {
        match row.kind {
            OperationKind::Deposit => {
                let amount = row
                    .amount
                    .ok_or("Deposit transaction is missing amount.")?
                    .trunc_with_scale(DECIMAL_PRECISION);

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

#[derive(Debug, Serialize)]
pub struct AccountRow {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
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
