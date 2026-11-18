use rust_decimal::Decimal;

pub const DECIMAL_PRECISION: u32 = 4;

// NOTE: could use a type alias, but this makes the intention clear
// at the time of construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u16);

// NOTE: could use a type alias, but this makes the intention clear
// at the time of construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(pub u32);

#[derive(Debug)]
pub enum DisputeKind {
    Open,
    Resolve,
    ChargeBack,
}

#[derive(Debug)]
pub struct Dispute {
    pub kind: DisputeKind,
    pub client: ClientId,
    pub tx: TxId,
}

#[derive(Debug, PartialEq)]
pub enum TransactionKind {
    Deposit,
    Withdrawal,
}

#[derive(Debug)]
pub struct Transaction {
    pub kind: TransactionKind,
    pub client: ClientId,
    pub tx: TxId,
    pub amount: Decimal,
}

#[derive(Debug)]
pub enum Operation {
    Transaction(Transaction),
    Dispute(Dispute),
}

#[derive(Debug)]
pub struct Account {
    pub client: ClientId,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}
