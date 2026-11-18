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
    pub fn transition_dispute_state(&mut self, dispute: &Dispute) {
        self.dispute_state = match dispute.kind {
            DisputeKind::Open => DisputeState::Disputed,
            DisputeKind::Resolve => DisputeState::NonDisputed,
            DisputeKind::ChargeBack => DisputeState::Reverted,
        };
    }
}

impl From<Transaction> for TransactionState {
    fn from(tx: Transaction) -> Self {
        Self {
            kind: tx.kind,
            client: tx.client,
            amount: tx.amount,
            dispute_state: DisputeState::default(),
        }
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
    pub fn apply_tx(&mut self, tx: &Transaction) -> Result<(), StateError> {
        match tx.kind {
            TransactionKind::Deposit => self._deposit(tx.amount),
            TransactionKind::Withdrawal => self._withdraw(tx.amount),
        }
    }

    pub fn apply_dispute(
        &mut self,
        tx: &mut TransactionState,
        dispute: &Dispute,
    ) -> Result<(), StateError> {
        // validate
        if !tx.is_dispute_transition_valid(dispute) {
            return Err(StateError::InvalidDisputeTransition);
        }

        // validate and mutate after
        match (&tx.kind, &dispute.kind) {
            (TransactionKind::Deposit, DisputeKind::Open) => self._dispute_deposit(tx.amount),
            (TransactionKind::Deposit, DisputeKind::Resolve) => self._resolve_deposit(tx.amount),
            (TransactionKind::Deposit, DisputeKind::ChargeBack) => {
                self._charge_back_deposit(tx.amount)
            }
            (TransactionKind::Withdrawal, DisputeKind::Open) => self._dispute_withdrawal(tx.amount),
            (TransactionKind::Withdrawal, DisputeKind::Resolve) => {
                self._resolve_withdrawal(tx.amount)
            }
            (TransactionKind::Withdrawal, DisputeKind::ChargeBack) => {
                self._charge_back_withdrawal(tx.amount)
            }
        }?;

        // mutate
        tx.transition_dispute_state(dispute);
        Ok(())
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
    fn _dispute_deposit(&mut self, amount: Decimal) -> Result<(), StateError> {
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
    fn _resolve_deposit(&mut self, amount: Decimal) -> Result<(), StateError> {
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
    fn _charge_back_deposit(&mut self, amount: Decimal) -> Result<(), StateError> {
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

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    fn _dispute_withdrawal(&mut self, amount: Decimal) -> Result<(), StateError> {
        let held = self
            .held
            .checked_add(amount)
            .ok_or(StateError::ArithmeticOverflow)?;
        let total = self
            .total
            .checked_add(amount)
            .ok_or(StateError::ArithmeticOverflow)?;

        self.held = held;
        self.total = total;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    ///
    /// TODO: held does never go below 0 due to the dispute transition logic,
    /// though I'd like to encapsulate invariant checking.
    fn _resolve_withdrawal(&mut self, amount: Decimal) -> Result<(), StateError> {
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

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    /// 2. chargeback locks account
    ///
    /// TODO: held does never go below 0 due to the dispute transition logic,
    /// though I'd like to encapsulate invariant checking.
    fn _charge_back_withdrawal(&mut self, amount: Decimal) -> Result<(), StateError> {
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
            client: *client,
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

        self.accounts.entry(tx.client).or_default().apply_tx(&tx)?;
        self.transactions.insert(tx.tx, TransactionState::from(tx));

        Ok(())
    }

    fn _apply_dispute(&mut self, dispute: Dispute) -> Result<(), StateError> {
        // TODO: can a locked account dispute? I think it should.
        //
        // Assumption: dispute.client == tx.client since if a client id can be wrong, we have
        // no way of determining which of the client ids is the right one.

        // 1. check if transaction exists
        let tx = self
            .transactions
            .get_mut(&dispute.tx)
            .ok_or(StateError::TransactionNotFound)?;

        // 2. this should never happen. Could unwrap, but this let's defensively return a recoverable error.
        let account = self
            .accounts
            .get_mut(&tx.client)
            .ok_or(StateError::AccountNotFound)?;

        account.apply_dispute(tx, &dispute)?;

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

    fn apply_dispute(
        transaction_kind: TransactionKind,
        state: DisputeState,
        kind: DisputeKind,
    ) -> (Result<DisputeState, StateError>, AccountState) {
        let mut transaction = TransactionState {
            kind: transaction_kind,
            client: ClientId(42),
            amount: dec!(10),
            dispute_state: state,
        };
        let dispute = Dispute {
            kind,
            client: ClientId(42),
            tx: TxId(123),
        };

        let mut account = AccountState {
            available: dec!(10),
            held: dec!(10),
            total: dec!(20),
            locked: false,
        };

        let result = account.apply_dispute(&mut transaction, &dispute);
        (result.map(|_| transaction.dispute_state), account)
    }

    fn assert_dispute_rejected(state: DisputeState, kind: DisputeKind) {
        let (result, account) = apply_dispute(TransactionKind::Deposit, state, kind);

        assert_eq!(result.unwrap_err(), StateError::InvalidDisputeTransition);
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(10), dec!(10), dec!(20), false)
        );
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

        account._dispute_deposit(dec!(3)).unwrap();
        assert_balances(&account, dec!(5), dec!(3), dec!(8));

        account._resolve_deposit(dec!(3)).unwrap();
        assert_balances(&account, dec!(8), dec!(0), dec!(8));
        assert!(!account.locked);

        account._dispute_deposit(dec!(4)).unwrap();
        assert_balances(&account, dec!(4), dec!(4), dec!(8));

        account._charge_back_deposit(dec!(4)).unwrap();
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
        let (result, account) = apply_dispute(
            TransactionKind::Deposit,
            DisputeState::NonDisputed,
            DisputeKind::Open,
        );

        assert!(matches!(result.unwrap(), DisputeState::Disputed));
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(0), dec!(20), dec!(20), false)
        );
    }

    #[test]
    fn resolving_disputed_transaction_marks_it_nondisputed() {
        let (result, account) = apply_dispute(
            TransactionKind::Deposit,
            DisputeState::Disputed,
            DisputeKind::Resolve,
        );

        assert!(matches!(result.unwrap(), DisputeState::NonDisputed));
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(20), dec!(0), dec!(20), false)
        );
    }

    #[test]
    fn charging_back_disputed_transaction_marks_it_reverted() {
        let (result, account) = apply_dispute(
            TransactionKind::Deposit,
            DisputeState::Disputed,
            DisputeKind::ChargeBack,
        );

        assert!(matches!(result.unwrap(), DisputeState::Reverted));
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(10), dec!(0), dec!(10), true)
        );
    }

    #[test]
    fn opening_withdrawal_dispute_adds_held_and_total() {
        let (result, account) = apply_dispute(
            TransactionKind::Withdrawal,
            DisputeState::NonDisputed,
            DisputeKind::Open,
        );

        assert!(matches!(result.unwrap(), DisputeState::Disputed));
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(10), dec!(20), dec!(30), false)
        );
    }

    #[test]
    fn resolving_withdrawal_dispute_subtracts_held_and_total() {
        let (result, account) = apply_dispute(
            TransactionKind::Withdrawal,
            DisputeState::Disputed,
            DisputeKind::Resolve,
        );

        assert!(matches!(result.unwrap(), DisputeState::NonDisputed));
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(10), dec!(0), dec!(10), false)
        );
    }

    #[test]
    fn charging_back_withdrawal_returns_held_funds_to_available() {
        let (result, account) = apply_dispute(
            TransactionKind::Withdrawal,
            DisputeState::Disputed,
            DisputeKind::ChargeBack,
        );

        assert!(matches!(result.unwrap(), DisputeState::Reverted));
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (dec!(20), dec!(0), dec!(20), true)
        );
    }

    #[test]
    fn withdrawal_dispute_total_overflow_leaves_account_unchanged() {
        let mut account = AccountState {
            available: Decimal::MAX,
            held: Decimal::ZERO,
            total: Decimal::MAX,
            locked: false,
        };
        let mut transaction = TransactionState {
            kind: TransactionKind::Withdrawal,
            client: ClientId(42),
            amount: Decimal::ONE,
            dispute_state: DisputeState::NonDisputed,
        };
        let dispute = Dispute {
            kind: DisputeKind::Open,
            client: ClientId(42),
            tx: TxId(123),
        };

        let result = account.apply_dispute(&mut transaction, &dispute);

        assert_eq!(result.unwrap_err(), StateError::ArithmeticOverflow);
        assert_eq!(
            (
                account.available,
                account.held,
                account.total,
                account.locked
            ),
            (Decimal::MAX, Decimal::ZERO, Decimal::MAX, false)
        );
    }

    #[test]
    fn resolving_nondisputed_transaction_fails_without_changing_account() {
        assert_dispute_rejected(DisputeState::NonDisputed, DisputeKind::Resolve);
    }

    #[test]
    fn charging_back_nondisputed_transaction_fails_without_changing_account() {
        assert_dispute_rejected(DisputeState::NonDisputed, DisputeKind::ChargeBack);
    }

    #[test]
    fn opening_disputed_transaction_fails_without_changing_account() {
        assert_dispute_rejected(DisputeState::Disputed, DisputeKind::Open);
    }

    #[test]
    fn opening_reverted_transaction_fails_without_changing_account() {
        assert_dispute_rejected(DisputeState::Reverted, DisputeKind::Open);
    }

    #[test]
    fn resolving_reverted_transaction_fails_without_changing_account() {
        assert_dispute_rejected(DisputeState::Reverted, DisputeKind::Resolve);
    }

    #[test]
    fn charging_back_reverted_transaction_fails_without_changing_account() {
        assert_dispute_rejected(DisputeState::Reverted, DisputeKind::ChargeBack);
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
    fn account(available: Decimal, held: Decimal, total: Decimal) -> AccountState {
        AccountState {
            available,
            held,
            total,
            locked: false,
        }
    }

    fn balances(account: &AccountState) -> (Decimal, Decimal, Decimal, bool) {
        (
            account.available,
            account.held,
            account.total,
            account.locked,
        )
    }

    fn assert_overflow_unchanged(
        operation: fn(&mut AccountState, Decimal) -> Result<(), StateError>,
        mut account: AccountState,
    ) {
        let before = balances(&account);
        assert_eq!(
            operation(&mut account, Decimal::ONE),
            Err(StateError::ArithmeticOverflow)
        );
        assert_eq!(balances(&account), before);
    }

    #[test]
    fn deposit_overflow_preserves_all_balances() {
        assert_overflow_unchanged(
            AccountState::_deposit,
            account(Decimal::MAX, Decimal::ZERO, Decimal::MAX),
        );
        assert_overflow_unchanged(
            AccountState::_deposit,
            account(Decimal::MAX - Decimal::ONE, Decimal::ONE, Decimal::MAX),
        );
    }

    #[test]
    fn withdrawal_arithmetic_limits_preserve_state() {
        let mut boundary = account(Decimal::MAX, Decimal::ZERO, Decimal::MAX);
        boundary._withdraw(Decimal::MAX).unwrap();
        assert_eq!(
            balances(&boundary),
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, false)
        );

        // Available cannot underflow: insufficient funds is checked before subtraction.
        assert_eq!(
            boundary._withdraw(Decimal::ONE),
            Err(StateError::InsufficientFunds)
        );
        assert_eq!(
            balances(&boundary),
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, false)
        );

        // Seed inconsistent internal balances to exercise the defensive total check.
        assert_overflow_unchanged(
            AccountState::_withdraw,
            account(Decimal::ONE, Decimal::ZERO, Decimal::MIN),
        );
    }

    #[test]
    fn deposit_dispute_overflow_preserves_all_balances() {
        assert_overflow_unchanged(
            AccountState::_dispute_deposit,
            account(Decimal::MIN, Decimal::MAX, Decimal::ZERO),
        );
        assert_overflow_unchanged(
            AccountState::_dispute_deposit,
            account(Decimal::MIN, Decimal::ZERO, Decimal::MIN),
        );
    }

    #[test]
    fn deposit_resolve_overflow_preserves_all_balances() {
        // Direct fixtures also cover arithmetic checks on inconsistent internal state.
        assert_overflow_unchanged(
            AccountState::_resolve_deposit,
            account(Decimal::MAX, Decimal::MIN, Decimal::ZERO),
        );
        assert_overflow_unchanged(
            AccountState::_resolve_deposit,
            account(Decimal::MAX, Decimal::ONE, Decimal::MAX),
        );
    }

    #[test]
    fn deposit_chargeback_overflow_preserves_all_balances_and_lock() {
        assert_overflow_unchanged(
            AccountState::_charge_back_deposit,
            account(Decimal::MAX, Decimal::MIN, Decimal::ZERO),
        );
        assert_overflow_unchanged(
            AccountState::_charge_back_deposit,
            account(Decimal::ZERO, Decimal::ONE, Decimal::MIN),
        );
    }

    #[test]
    fn withdrawal_dispute_overflow_preserves_all_balances() {
        assert_overflow_unchanged(
            AccountState::_dispute_withdrawal,
            account(Decimal::ZERO, Decimal::MAX, Decimal::MAX),
        );
        assert_overflow_unchanged(
            AccountState::_dispute_withdrawal,
            account(Decimal::MAX, Decimal::ZERO, Decimal::MAX),
        );
    }

    #[test]
    fn withdrawal_resolve_overflow_preserves_all_balances() {
        assert_overflow_unchanged(
            AccountState::_resolve_withdrawal,
            account(Decimal::ZERO, Decimal::MIN, Decimal::MIN),
        );
        assert_overflow_unchanged(
            AccountState::_resolve_withdrawal,
            account(Decimal::ZERO, Decimal::ONE, Decimal::MIN),
        );
    }

    #[test]
    fn withdrawal_chargeback_overflow_preserves_all_balances_and_lock() {
        assert_overflow_unchanged(
            AccountState::_charge_back_withdrawal,
            account(Decimal::MAX, Decimal::MIN, Decimal::ZERO),
        );
        assert_overflow_unchanged(
            AccountState::_charge_back_withdrawal,
            account(Decimal::MAX, Decimal::ONE, Decimal::MAX),
        );
    }

    #[test]
    fn rejected_dispute_preserves_transaction_history() {
        let mut store = Store::new();
        store
            .apply_operation(Operation::Transaction(Transaction {
                kind: TransactionKind::Deposit,
                client: ClientId(1),
                tx: TxId(1),
                amount: Decimal::ONE,
            }))
            .unwrap();
        let before = balances(store.accounts.get(&ClientId(1)).unwrap());

        assert_eq!(
            store.apply_operation(Operation::Dispute(Dispute {
                kind: DisputeKind::Resolve,
                client: ClientId(1),
                tx: TxId(1),
            })),
            Err(StateError::InvalidDisputeTransition)
        );
        assert_eq!(balances(store.accounts.get(&ClientId(1)).unwrap()), before);
        assert!(
            store.transactions.contains_key(&TxId(1)),
            "rejection must retain transaction history"
        );
    }

    #[test]
    fn dispute_overflow_preserves_transaction_history() {
        let mut store = Store::new();
        store.accounts.insert(
            ClientId(1),
            account(Decimal::MAX, Decimal::ZERO, Decimal::MAX),
        );
        store.transactions.insert(
            TxId(1),
            TransactionState {
                kind: TransactionKind::Withdrawal,
                client: ClientId(1),
                amount: Decimal::ONE,
                dispute_state: DisputeState::NonDisputed,
            },
        );
        let before = balances(store.accounts.get(&ClientId(1)).unwrap());

        assert_eq!(
            store.apply_operation(Operation::Dispute(Dispute {
                kind: DisputeKind::Open,
                client: ClientId(1),
                tx: TxId(1),
            })),
            Err(StateError::ArithmeticOverflow)
        );
        assert_eq!(balances(store.accounts.get(&ClientId(1)).unwrap()), before);
        assert!(
            store.transactions.contains_key(&TxId(1)),
            "overflow must retain transaction history"
        );
    }

    #[test]
    fn missing_account_preserves_transaction_history() {
        let mut store = Store::new();
        store.transactions.insert(
            TxId(1),
            TransactionState {
                kind: TransactionKind::Deposit,
                client: ClientId(1),
                amount: Decimal::ONE,
                dispute_state: DisputeState::NonDisputed,
            },
        );

        assert_eq!(
            store.apply_operation(Operation::Dispute(Dispute {
                kind: DisputeKind::Open,
                client: ClientId(1),
                tx: TxId(1),
            })),
            Err(StateError::AccountNotFound)
        );
        assert!(store.accounts.is_empty());
        assert!(
            store.transactions.contains_key(&TxId(1)),
            "missing account must not discard history"
        );
    }
}
