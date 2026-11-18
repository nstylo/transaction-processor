# Transaction Processing Engine

## Build the Project

```sh
cargo build
```

## Run the Project

```sh
cargo run -- transactions.csv > accounts.csv
```

## Test the Project

The project contains a fully fledged integration test suite which tests the whole flow from reading in a transactions csv to writing the final accounts csv using file based fixtures. Critical sections in the code are unit tested. For testing the whole project run:

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

## Robustness

- No unwraps have been used in non-test code
- Critical IO errors will cause the program to return the error and terminate
- Per operation errors are logged to stderr and don't cause program termination

## Future Work

- [ ] Abstract account store using a trait, so we can plug in another storage engine other than the ephemeral in memory storage we currently provide.
- [ ] Support unsigned decimals and if possible make the precision part of the type
- [ ] Handle IO errors more gracefully -> retries, backoff, etc.
- [ ] Assume system to be distributed and inputs/outputs other than filesystem. There are reader/writer abstractions but we don't support any other inputs yet.
