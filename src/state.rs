use rust_decimal::{Decimal, dec};
use std::collections::HashMap;
use thiserror::Error;

use crate::domain::{
    Account, ClientId, Dispute, DisputeKind, Operation, Transaction, TransactionKind, TxId,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateError {
    #[error("invalid dispute state transition")]
    InvalidDisputeTransition,
    #[error("decimal arithmetic overflow")]
    ArithmeticOverflow,
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error("transaction amount must be positive")]
    NonPositiveAmount,
    #[error("duplicate transaction ID")]
    DuplicateTransactionId,
    #[error("account is locked")]
    AccountLocked,
    #[error("transaction not found")]
    TransactionNotFound,
    #[error("account not found for transaction")]
    AccountNotFound,
}

#[derive(Debug, Default)]
enum DisputeState {
    #[default]
    NonDisputed,
    Disputed,
    Reverted,
}

#[derive(Debug)]
struct TransactionState {
    // NOTE: the exercise does not specify the case in which a withdraw transaction
    // is disputed. We will ha
    kind: TransactionKind,
    client: ClientId,
    amount: Decimal,
    dispute_state: DisputeState,
}

impl TransactionState {
    pub fn is_dispute_transition_valid(&self, dispute: &Dispute) -> bool {
        matches!(
            (&self.dispute_state, &dispute.kind),
            (DisputeState::NonDisputed, DisputeKind::Open)
                | (
                    DisputeState::Disputed,
                    DisputeKind::Resolve | DisputeKind::ChargeBack
                )
        )
    }

    /// IMPORTANT: validate if transition is valid, before applying it.
    pub fn transition_dispute_state(mut self, dispute: &Dispute) -> Self {
        self.dispute_state = match dispute.kind {
            DisputeKind::Open => DisputeState::Disputed,
            DisputeKind::Resolve => DisputeState::NonDisputed,
            DisputeKind::ChargeBack => DisputeState::Reverted,
        };

        self
    }
}

#[derive(Debug, Default)]
struct AccountState {
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

impl AccountState {
    pub fn apply_tx(&mut self, tx: Transaction) -> Result<(), StateError> {
        match tx.kind {
            TransactionKind::Deposit => self._deposit(tx.amount),
            TransactionKind::Withdrawal => self._withdraw(tx.amount),
        }
    }

    pub fn apply_dispute(
        &mut self,
        tx: TransactionState,
        dispute: &Dispute,
    ) -> Result<TransactionState, StateError> {
        // validate
        if !tx.is_dispute_transition_valid(&dispute) {
            return Err(StateError::InvalidDisputeTransition);
        }

        // validate and mutate after
        match dispute.kind {
            DisputeKind::Open => self._dispute(tx.amount),
            DisputeKind::Resolve => self._resolve(tx.amount),
            DisputeKind::ChargeBack => self._charge_back(tx.amount),
        }?;

        // mutate
        Ok(tx.transition_dispute_state(&dispute))
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    fn _deposit(&mut self, amount: Decimal) -> Result<(), StateError> {
        let available = self
            .available
            .checked_add(amount)
            .ok_or(StateError::ArithmeticOverflow)?;
        let total = self
            .total
            .checked_add(amount)
            .ok_or(StateError::ArithmeticOverflow)?;

        self.available = available;
        self.total = total;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. available >= 0
    /// 2. total = available + held
    fn _withdraw(&mut self, amount: Decimal) -> Result<(), StateError> {
        if self.available < amount {
            return Err(StateError::InsufficientFunds);
        }

        let available = self
            .available
            .checked_sub(amount)
            .ok_or(StateError::ArithmeticOverflow)?;
        let total = self
            .total
            .checked_sub(amount)
            .ok_or(StateError::ArithmeticOverflow)?;

        self.available = available;
        self.total = total;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    ///
    /// NOTE: available can get negative here.
    fn _dispute(&mut self, amount: Decimal) -> Result<(), StateError> {
        let held = self
            .held
            .checked_add(amount)
            .ok_or(StateError::ArithmeticOverflow)?;
        let available = self
            .available
            .checked_sub(amount)
            .ok_or(StateError::ArithmeticOverflow)?;

        self.held = held;
        self.available = available;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    ///
    /// TODO: held does never go below 0 due to the dispute transition logic,
    /// though I'd like to encapsulate invariant checking.
    fn _resolve(&mut self, amount: Decimal) -> Result<(), StateError> {
        let held = self
            .held
            .checked_sub(amount)
            .ok_or(StateError::ArithmeticOverflow)?;
        let available = self
            .available
            .checked_add(amount)
            .ok_or(StateError::ArithmeticOverflow)?;

        self.held = held;
        self.available = available;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    /// 2. chargeback locks account
    ///
    /// TODO: held does never go below 0 due to the dispute transition logic,
    /// though I'd like to encapsulate invariant checking.
    /// NOTE: total can get negative here.
    fn _charge_back(&mut self, amount: Decimal) -> Result<(), StateError> {
        let held = self
            .held
            .checked_sub(amount)
            .ok_or(StateError::ArithmeticOverflow)?;
        let total = self
            .total
            .checked_sub(amount)
            .ok_or(StateError::ArithmeticOverflow)?;

        self.held = held;
        self.total = total;
        self.locked = true;

        Ok(())
    }
}

// TODO: we wanna abstract over state store -> trait?
// The below is an in mem version of the state store
#[derive(Default)]
pub struct Store {
    // Assumption: one account per client, so client id maps directly to account state
    accounts: HashMap<ClientId, AccountState>,
    transactions: HashMap<TxId, TransactionState>,
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    pub fn apply_operation(&mut self, op: Operation) -> Result<(), StateError> {
        match op {
            Operation::Transaction(transaction) => self._apply_tx(transaction),
            Operation::Dispute(dispute) => self._apply_dispute(dispute),
        }
    }

    /// Note: clones internal state
    pub fn accounts(&self) -> impl Iterator<Item = Account> {
        self.accounts.iter().map(|(client, state)| Account {
            client: client.clone(),
            available: state.available,
            held: state.held,
            total: state.total,
            locked: state.locked,
        })
    }

    fn _apply_tx(&mut self, tx: Transaction) -> Result<(), StateError> {
        // 1. Allow only positive decimals
        // TODO: not sure about 0 amount transactions, either way it wouldn't change correctness.
        // TODO: ideally pass unsigned value, and validate at the input boundary
        if tx.amount <= dec!(0) {
            return Err(StateError::NonPositiveAmount);
        }

        // 2. don't allow duplicate transactions
        if self.transactions.contains_key(&tx.tx) {
            return Err(StateError::DuplicateTransactionId);
        }

        // 3. check if account is locked if it exists
        if self
            .accounts
            .get(&tx.client)
            .is_some_and(|account| account.locked)
        {
            return Err(StateError::AccountLocked);
        }

        self.accounts.entry(tx.client).or_default().apply_tx(tx)
    }

    fn _apply_dispute(&mut self, dispute: Dispute) -> Result<(), StateError> {
        // TODO: can a locked account dispute? I think it should.

        // 1. check if transaction exists
        let tx = self
            .transactions
            .remove(&dispute.tx)
            .ok_or(StateError::TransactionNotFound)?;

        // 2. this should never happen. Could unwrap, but this let's defensively return a recoverable error.
        let account = self
            .accounts
            .get_mut(&tx.client)
            .ok_or(StateError::AccountNotFound)?;

        let tx = account.apply_dispute(tx, &dispute)?;
        self.transactions.insert(dispute.tx, tx);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        ClientId, Dispute, DisputeKind, Operation, Transaction, TransactionKind, TxId,
    };

    use super::{AccountState, DisputeState, StateError, Store, TransactionState};
    use rust_decimal::{Decimal, dec};

    fn transaction(kind: TransactionKind, amount: Decimal) -> Operation {
        Operation::Transaction(Transaction {
            kind,
            client: ClientId(42),
            tx: TxId(123),
            amount,
        })
    }

    fn dispute(kind: DisputeKind) -> Operation {
        Operation::Dispute(Dispute {
            kind,
            client: ClientId(42),
            tx: TxId(123),
        })
    }

    fn store_with_transaction() -> Store {
        let mut store = Store::new();
        store.transactions.insert(
            TxId(123),
            TransactionState {
                kind: TransactionKind::Deposit,
                client: ClientId(42),
                amount: dec!(10),
                dispute_state: DisputeState::NonDisputed,
            },
        );
        store
    }

    fn advance_dispute(
        state: DisputeState,
        kind: DisputeKind,
    ) -> (Result<(), StateError>, DisputeState) {
        let mut transaction = TransactionState {
            kind: TransactionKind::Deposit,
            client: ClientId(42),
            amount: dec!(10),
            dispute_state: state,
        };
        let dispute = Dispute {
            kind,
            client: ClientId(42),
            tx: TxId(123),
        };

        let result = transaction.advance_dispute_state(&dispute);
        (result, transaction.dispute_state)
    }

    #[test]
    fn account_operations_update_balances() {
        let mut account = AccountState::default();
        let assert_balances = |account: &AccountState, available, held, total| {
            assert_eq!(
                (account.available, account.held, account.total),
                (available, held, total)
            );
        };

        account._deposit(dec!(10.1234)).unwrap();
        assert_balances(&account, dec!(10.1234), dec!(0), dec!(10.1234));

        account._withdraw(dec!(2.1234)).unwrap();
        assert_balances(&account, dec!(8), dec!(0), dec!(8));

        account._dispute(dec!(3)).unwrap();
        assert_balances(&account, dec!(5), dec!(3), dec!(8));

        account._resolve(dec!(3)).unwrap();
        assert_balances(&account, dec!(8), dec!(0), dec!(8));
        assert!(!account.locked);

        account._dispute(dec!(4)).unwrap();
        assert_balances(&account, dec!(4), dec!(4), dec!(8));

        account._charge_back(dec!(4)).unwrap();
        assert_balances(&account, dec!(4), dec!(0), dec!(4));
        assert!(account.locked);
    }

    #[test]
    fn total_overflow_leaves_account_balances_unchanged() {
        let available = Decimal::MAX - Decimal::ONE;
        let mut account = AccountState {
            available,
            held: Decimal::ONE,
            total: Decimal::MAX,
            locked: false,
        };

        assert_eq!(
            account._deposit(Decimal::ONE),
            Err(StateError::ArithmeticOverflow)
        );
        assert_eq!(account.available, available);
        assert_eq!(account.held, Decimal::ONE);
        assert_eq!(account.total, Decimal::MAX);
        assert!(!account.locked);
    }

    #[test]
    fn opening_nondisputed_transaction_marks_it_disputed() {
        let (result, state) = advance_dispute(DisputeState::NonDisputed, DisputeKind::Open);

        assert_eq!(result, Ok(()));
        assert!(matches!(state, DisputeState::Disputed));
    }

    #[test]
    fn resolving_disputed_transaction_marks_it_nondisputed() {
        let (result, state) = advance_dispute(DisputeState::Disputed, DisputeKind::Resolve);

        assert_eq!(result, Ok(()));
        assert!(matches!(state, DisputeState::NonDisputed));
    }

    #[test]
    fn charging_back_disputed_transaction_marks_it_charged_back() {
        let (result, state) = advance_dispute(DisputeState::Disputed, DisputeKind::ChargeBack);

        assert_eq!(result, Ok(()));
        assert!(matches!(state, DisputeState::ChargeBack));
    }

    #[test]
    fn resolving_nondisputed_transaction_fails_without_changing_state() {
        let (result, state) = advance_dispute(DisputeState::NonDisputed, DisputeKind::Resolve);

        assert_eq!(result, Err(StateError::InvalidDisputeTransition));
        assert!(matches!(state, DisputeState::NonDisputed));
    }

    #[test]
    fn charging_back_nondisputed_transaction_fails_without_changing_state() {
        let (result, state) = advance_dispute(DisputeState::NonDisputed, DisputeKind::ChargeBack);

        assert_eq!(result, Err(StateError::InvalidDisputeTransition));
        assert!(matches!(state, DisputeState::NonDisputed));
    }

    #[test]
    fn opening_disputed_transaction_fails_without_changing_state() {
        let (result, state) = advance_dispute(DisputeState::Disputed, DisputeKind::Open);

        assert_eq!(result, Err(StateError::InvalidDisputeTransition));
        assert!(matches!(state, DisputeState::Disputed));
    }

    #[test]
    fn opening_charged_back_transaction_fails_without_changing_state() {
        let (result, state) = advance_dispute(DisputeState::ChargeBack, DisputeKind::Open);

        assert_eq!(result, Err(StateError::InvalidDisputeTransition));
        assert!(matches!(state, DisputeState::ChargeBack));
    }

    #[test]
    fn resolving_charged_back_transaction_fails_without_changing_state() {
        let (result, state) = advance_dispute(DisputeState::ChargeBack, DisputeKind::Resolve);

        assert_eq!(result, Err(StateError::InvalidDisputeTransition));
        assert!(matches!(state, DisputeState::ChargeBack));
    }

    #[test]
    fn charging_back_charged_back_transaction_fails_without_changing_state() {
        let (result, state) = advance_dispute(DisputeState::ChargeBack, DisputeKind::ChargeBack);

        assert_eq!(result, Err(StateError::InvalidDisputeTransition));
        assert!(matches!(state, DisputeState::ChargeBack));
    }

    #[test]
    fn invalid_dispute_transition_returns_typed_error() {
        let mut store = store_with_transaction();
        store.accounts.insert(ClientId(42), AccountState::default());

        assert_eq!(
            store.apply_operation(dispute(DisputeKind::Resolve)),
            Err(StateError::InvalidDisputeTransition)
        );
    }

    #[test]
    fn balance_overflow_returns_typed_error() {
        let mut store = Store::new();
        store.accounts.insert(
            ClientId(42),
            AccountState {
                available: Decimal::MAX,
                total: Decimal::MAX,
                ..AccountState::default()
            },
        );

        assert_eq!(
            store.apply_operation(transaction(TransactionKind::Deposit, Decimal::ONE)),
            Err(StateError::ArithmeticOverflow)
        );
    }

    #[test]
    fn insufficient_funds_returns_typed_error() {
        let mut store = Store::new();

        assert_eq!(
            store.apply_operation(transaction(TransactionKind::Withdrawal, Decimal::ONE)),
            Err(StateError::InsufficientFunds)
        );
    }

    #[test]
    fn nonpositive_amount_returns_typed_error() {
        let mut store = Store::new();

        for amount in [dec!(-1), Decimal::ZERO] {
            assert_eq!(
                store.apply_operation(transaction(TransactionKind::Deposit, amount)),
                Err(StateError::NonPositiveAmount)
            );
        }
    }

    #[test]
    fn duplicate_transaction_id_returns_typed_error() {
        let mut store = store_with_transaction();

        assert_eq!(
            store.apply_operation(transaction(TransactionKind::Deposit, Decimal::ONE)),
            Err(StateError::DuplicateTransactionId)
        );
    }

    #[test]
    fn locked_account_returns_typed_error() {
        let mut store = Store::new();
        store.accounts.insert(
            ClientId(42),
            AccountState {
                locked: true,
                ..AccountState::default()
            },
        );

        assert_eq!(
            store.apply_operation(transaction(TransactionKind::Deposit, Decimal::ONE)),
            Err(StateError::AccountLocked)
        );
    }

    #[test]
    fn missing_transaction_returns_typed_error() {
        let mut store = Store::new();

        assert_eq!(
            store.apply_operation(dispute(DisputeKind::Open)),
            Err(StateError::TransactionNotFound)
        );
    }

    #[test]
    fn missing_account_returns_typed_error() {
        let mut store = store_with_transaction();

        assert_eq!(
            store.apply_operation(dispute(DisputeKind::Open)),
            Err(StateError::AccountNotFound)
        );
    }
}
