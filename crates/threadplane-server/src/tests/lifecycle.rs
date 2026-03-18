use super::*;

#[rstest]
#[case(None, 300, 300)]
#[case(Some(5), 300, MINIMUM_LEASE_SECONDS)]
#[case(Some(30), 300, 30)]
#[case(Some(120), 300, 120)]
fn normalized_lease_seconds_enforces_minimum(
    #[case] requested_lease_seconds: Option<i64>,
    #[case] default_lease_seconds: i64,
    #[case] expected: i64,
) {
    assert_eq!(
        normalized_lease_seconds(requested_lease_seconds, default_lease_seconds),
        expected
    );
}

#[rstest]
#[case(None, 25)]
#[case(Some(0), 1)]
#[case(Some(7), 7)]
#[case(Some(500), 200)]
fn normalized_list_limit_clamps_bounds(#[case] requested: Option<i64>, #[case] expected: i64) {
    assert_eq!(normalized_list_limit(requested), expected);
}

proptest::proptest! {
    #[test]
    fn calculated_claim_expiry_advances_by_requested_seconds(lease_seconds in MINIMUM_LEASE_SECONDS..10_000_i64) {
        let claimed_at = Utc::now();
        let expires_at = calculate_claim_expiry(claimed_at, lease_seconds);

        prop_assert_eq!(
            expires_at.map(|value| value.signed_duration_since(claimed_at).num_seconds()),
            Some(lease_seconds)
        );
    }
}

#[tokio::test]
async fn wait_for_shutdown_completes_after_cancellation() {
    let shutdown_token = CancellationToken::new();
    shutdown_token.cancel();

    wait_for_shutdown(shutdown_token).await;
}
