# Transaction Processing Engine

```sh
cargo run -- transactions.csv > accounts.csv
```

## Assumptions

1. `client` field is unqiue per client
2. `tx` field is globally unique across all rows
3. input rows are not ordered by `client`
4. input rows are not ordered by `tx`
5. input rows are ordered chronologically
6. transactions reference clients; if `client` doesn't exist, create record
7. transactions only consider a single asset account per `client`

## Correctness Invariants

1. for a given account: `total` = `available` + `held`
2. for a given withdrawal: `account.available` >= `withdrawal.amount`
3.

## Open Questions

1. What does 'locked' mean? Does it prevent any further transactions for this account?
2. Are negative account totals allowed? Apparently yes.

## Data Modelling

### Account

```rust
struct Account {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}
```

### Transaction

A transaction is a state transition to Account. I present two alternative approaches:

#### Separate TransactionType Enum

```rust
enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
}

struct Transaction {
  type: TransactionType,
  client: u16,
  tx: u32,
  amount: Option<Decimal>,
}
```

#### Enum carrying data

```rust
enum Transaction {
    Deposit {
        client: u16,
        tx: u32,
        amount: Decimal,
    },
    Withdrawal {
        client: u16,
        tx: u32,
        amount: Decimal,
    },
    Dispute {
        client: u16,
        tx: u32,
    },
    Resolve {
        client: u16,
        tx: u32,
    }
}
```

### Stored Transaction State

```rust
enum DisputeState {
    Undisputed,
    Disputed,
    Resolved,
    Backcharged,
}

struct StoredTransaction {
    tx: Transaction,
    dispute_state: DisputeState,
}
```
