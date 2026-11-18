use rust_decimal::{Decimal, dec};
use std::collections::HashMap;

use crate::{ClientId, Dispute, DisputeKind, Operation, Transaction, TransactionKind, TxId};

#[derive(Debug, Default)]
enum DisputeState {
    #[default]
    NonDisputed,
    Disputed,
    ChargeBack,
}

#[derive(Debug)]
struct TransactionState {
    kind: TransactionKind,
    client: ClientId,
    amount: Decimal,
    dispute_state: DisputeState,
}

impl TransactionState {
    /// finite state machine for dispute state:
    pub fn advance_dispute_state(&mut self, dispute: &Dispute) -> Result<(), &'static str> {
        let new_state = match (&self.dispute_state, &dispute.kind) {
            (DisputeState::NonDisputed, DisputeKind::Open) => Ok(DisputeState::Disputed),
            (DisputeState::Disputed, DisputeKind::Resolve) => Ok(DisputeState::NonDisputed),
            (DisputeState::Disputed, DisputeKind::ChargeBack) => Ok(DisputeState::ChargeBack),
            (_, _) => Err("Invariant Error: Invalid Dispute Type."),
        }?;

        self.dispute_state = new_state;
        Ok(())
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
    pub fn apply_tx(&mut self, tx: Transaction) -> Result<(), &'static str> {
        match tx.kind {
            TransactionKind::Deposit => self._deposit(tx.amount),
            TransactionKind::Withdrawal => self._withdraw(tx.amount),
        }
    }

    // pub fn apply_dispute(&mut self, dispute: Dispute) -> Result<(), &'static str> {
    //     match dispute.kind {
    //         DisputeKind::Open => self._dispute(),
    //     }
    // }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    fn _deposit(&mut self, amount: Decimal) -> Result<(), &'static str> {
        self.available
            .checked_add(amount)
            .ok_or("Critical Error: Overflow.")?;
        self.total
            .checked_add(amount)
            .ok_or("Critical Error: Overflow.")?;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. available >= 0
    /// 2. total = available + held
    fn _withdraw(&mut self, amount: Decimal) -> Result<(), &'static str> {
        if self.available < amount {
            return Err("Invariant Error: amount > available.");
        }

        self.available
            .checked_sub(amount)
            .ok_or("Critical Error: Overflow.")?;
        self.total
            .checked_sub(amount)
            .ok_or("Critical Error: Overflow.")?;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    ///
    /// NOTE: available can get negative here.
    fn _dispute(&mut self, amount: Decimal) -> Result<(), &'static str> {
        self.held
            .checked_add(amount)
            .ok_or("Critical Error: Overflow.")?;
        self.available
            .checked_sub(amount)
            .ok_or("Critical Error: Overflow.")?;

        Ok(())
    }

    /// Maintain invariants:
    ///
    /// 1. total = available + held
    ///
    /// TODO: held does never go below 0 due to the dispute transition logic,
    /// though I'd like to encapsulate invariant checking.
    fn _resolve(&mut self, amount: Decimal) -> Result<(), &'static str> {
        self.held
            .checked_sub(amount)
            .ok_or("Critical Error: Overflow.")?;
        self.available
            .checked_add(amount)
            .ok_or("Critical Error: Overflow.")?;

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
    fn _charge_back(&mut self, amount: Decimal) -> Result<(), &'static str> {
        self.held
            .checked_sub(amount)
            .ok_or("Critical Error: Overflow.")?;
        self.total
            .checked_sub(amount)
            .ok_or("Critical Error: Overflow.")?;
        self.locked = true;

        Ok(())
    }
}

// TODO: we wanna abstract over state store -> trait?
// The below is an in mem version of the state store
// TODO: I kinda want to
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

    pub fn apply_operation(&mut self, op: Operation) -> Result<(), &'static str> {
        match op {
            Operation::Transaction(transaction) => self._apply_tx(transaction),
            Operation::Dispute(dispute) => self._apply_dispute(dispute),
        }
    }

    fn _apply_tx(&mut self, tx: Transaction) -> Result<(), &'static str> {
        // 1. Allow only positive decimals
        // TODO: not sure about 0 amount transactions, either way it wouldn't change correctness.
        // TODO: ideally pass unsigned value, and validate at the input boundary
        if tx.amount <= dec!(0) {
            return Err("Invariant Error: Amount needs to be positive.");
        }

        // 2. don't allow duplicate transactions
        if self.transactions.contains_key(&tx.tx) {
            return Err("Invariant Error: Duplicate tx id.");
        }

        // 3. check if account is locked if it exists
        if self
            .accounts
            .get(&tx.client)
            .is_some_and(|account| account.locked)
        {
            return Err("Invariant Error: Account locked.");
        }

        match tx.kind {
            TransactionKind::Deposit => self
                .accounts
                .entry(tx.client)
                .or_default()
                ._deposit(tx.amount),
            TransactionKind::Withdrawal => self
                .accounts
                // this will create an account even if _withdraw fails. Doesn't hurt correctness.
                .entry(tx.client)
                .or_default()
                ._withdraw(tx.amount),
        }
    }

    fn _apply_dispute(&mut self, dispute: Dispute) -> Result<(), &'static str> {
        // TODO: can a locked account dispute? I think it should.

        // 1. check if transaction exists
        let tx = self
            .transactions
            .get_mut(&dispute.tx)
            .ok_or("Invariant Error: Transaction couldn't be found.")?;

        // 2. this should never happen. Could unwrap, but this let's defensively return a recoverable error.
        let account = self
            .accounts
            .get_mut(&tx.client)
            .ok_or("Critical Error: No account for this transaction.")?;

        // 3. check if dispute transition is valid
        tx.advance_dispute_state(&dispute)?;

        match dispute.kind {
            DisputeKind::Open => account._dispute(tx.amount),
            DisputeKind::Resolve => account._resolve(tx.amount),
            DisputeKind::ChargeBack => account._charge_back(tx.amount),
        }
    }
}
