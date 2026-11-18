# Transaction Processing Engine

## Run the Project

```sh
cargo run -- transactions.csv > accounts.csv
```

## Test the Project

```sh
cargo test
```

## Assumptions

1. `client` field is unqiue per client
2. `tx` field is globally unique across all rows
3. input rows are not ordered by `client`
4. input rows are not ordered by `tx`
5. input rows are ordered chronologically
6. transactions reference clients; if `client` doesn't exist, create record
7. transactions only consider a single asset account per `client`
8. `client` is consistent for a given tx and its dispute op
9. withdraw disputes are supported (although not explicitly specified in the doc)
10. 0 transactions are rejected and are not stored
11. input decimal precision is assumed to be 4, we simply truncate to that precision

## Correctness Invariants for Accounts

1. `account.total` = `account.available` + `account.held`
2. for a given withdrawal: `account.available` >= `withdrawal.amount`
