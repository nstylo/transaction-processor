use rust_decimal::Decimal;
use serde::Deserialize;
use std::{fs, io::Cursor, path::Path};
use transaction_processor::{Store, run};

#[derive(Debug, Deserialize, PartialEq)]
struct AccountRecord {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

fn assert_fixture(name: &str) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let input =
        fs::read(fixture.join("transactions.csv")).expect("transactions fixture should load");
    let control = fs::read(fixture.join("accounts.csv")).expect("accounts control should load");
    let mut store = Store::new();
    let mut output = Vec::new();

    run(Cursor::new(input), &mut output, &mut store).expect("fixture should run");

    assert_eq!(
        account_records(&output),
        account_records(&control),
        "{name}"
    );
}

fn account_records(input: &[u8]) -> Vec<AccountRecord> {
    let mut reader = csv::Reader::from_reader(input);
    assert_eq!(
        reader
            .headers()
            .expect("account CSV should contain a header"),
        &csv::StringRecord::from(vec!["client", "available", "held", "total", "locked"])
    );

    let mut accounts: Vec<AccountRecord> = reader
        .deserialize()
        .collect::<Result<_, _>>()
        .expect("account CSV should deserialize");
    accounts.sort_by_key(|account| account.client);
    accounts
}

#[test]
fn deposits_and_withdrawals_produce_account_csv_in_memory() {
    assert_fixture("basic");
}

#[test]
fn deposit_disputes_produce_account_csv_in_memory() {
    assert_fixture("deposit_disputes");
}

#[test]
fn withdrawal_disputes_produce_account_csv_in_memory() {
    assert_fixture("withdrawal_disputes");
}

#[test]
fn rejected_operations_do_not_prevent_processing_later_rows() {
    assert_fixture("rejected_operations");
}
