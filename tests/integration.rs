use std::{fs, io::Cursor, path::Path};
use transaction_processor::{Store, run, serde::AccountRow};

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

fn account_records(input: &[u8]) -> Vec<AccountRow> {
    let mut reader = csv::Reader::from_reader(input);
    let mut accounts: Vec<AccountRow> = reader
        .deserialize()
        .collect::<Result<_, _>>()
        .expect("account CSV should deserialize");
    accounts.sort_by_key(|account| account.client);
    accounts
}

macro_rules! fixture_tests {
    ($($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() {
                assert_fixture(stringify!($case));
            }
        )*
    };
}

fixture_tests!(
    basic,
    deposit_disputes,
    withdrawal_disputes,
    rejected_operations,
    simple_condensed,
    simple_whitespace_truncated,
    complex_without_disputes,
    deposit_dispute_resolve,
    deposit_dispute_chargeback,
    withdrawal_dispute_resolve,
    withdrawal_dispute_chargeback,
    complex_multiclient_disputes,
    deposit_dispute_reopened,
    withdrawal_dispute_reopened,
    simultaneous_disputes,
    mixed_disputes_same_account,
    partial_resolution_retains_other_hold,
    partial_chargeback_retains_other_hold,
    negative_available_after_deposit_dispute,
    negative_total_after_deposit_chargeback,
    withdrawal_exact_available,
    minimum_positive_amount,
    large_decimal_precision,
    truncation_carry_boundaries,
    boundary_ids,
    interleaved_nonmonotonic_ids,
    missing_deposit_amount,
    missing_withdrawal_amount,
    zero_deposit,
    zero_withdrawal,
    negative_deposit,
    negative_withdrawal,
    deposit_truncated_to_zero,
    withdrawal_truncated_to_zero,
    withdrawal_exceeds_available,
    held_funds_not_withdrawable,
    withdrawal_before_funding,
    duplicate_deposit_id,
    duplicate_withdrawal_id,
    withdrawal_reuses_deposit_id,
    deposit_reuses_withdrawal_id,
    duplicate_id_across_clients,
    rejected_transaction_id_reusable,
    missing_amount_id_reusable,
    resolved_transaction_id_reserved,
    reverted_transaction_id_reserved,
    deposit_into_locked_account,
    withdrawal_from_locked_account,
    dispute_unknown_transaction,
    resolve_unknown_transaction,
    chargeback_unknown_transaction,
    deposit_resolve_before_dispute_retains_history,
    deposit_chargeback_before_dispute_retains_history,
    deposit_duplicate_dispute_retains_history,
    deposit_repeated_resolve_retains_history,
    deposit_dispute_after_chargeback_retains_history,
    deposit_resolve_after_chargeback_retains_history,
    deposit_chargeback_after_chargeback_retains_history,
    deposit_dispute_amount_ignored,
    deposit_resolve_amount_ignored,
    deposit_chargeback_amount_ignored,
    withdrawal_resolve_before_dispute_retains_history,
    withdrawal_chargeback_before_dispute_retains_history,
    withdrawal_duplicate_dispute_retains_history,
    withdrawal_repeated_resolve_retains_history,
    withdrawal_dispute_after_chargeback_retains_history,
    withdrawal_resolve_after_chargeback_retains_history,
    withdrawal_chargeback_after_chargeback_retains_history,
    withdrawal_dispute_amount_ignored,
    withdrawal_resolve_amount_ignored,
    withdrawal_chargeback_amount_ignored,
    empty_input,
    header_only_input,
    all_operations_rejected,
    crlf_input,
    no_final_newline,
    blank_lines,
    quoted_fields,
    output_schema_and_precision,
    failed_first_withdrawal_creates_zero_account,
    locked_account_deposit_dispute,
    locked_account_deposit_resolve,
    locked_account_deposit_chargeback,
    locked_account_withdrawal_dispute,
    locked_account_withdrawal_resolve,
    locked_account_withdrawal_chargeback,
);
