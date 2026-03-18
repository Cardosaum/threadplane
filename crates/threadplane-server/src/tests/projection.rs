use super::*;

#[test]
fn projection_status_marks_caught_up_when_pending_is_zero() {
    let created_at = Utc::now();
    let cursor = ProjectionCursor::new(created_at, Uuid::nil());
    let status = build_projection_status("neo4j_graph", Some(cursor), 12, 0);

    assert!(status.caught_up);
    assert_eq!(status.projected_events, 12);
    assert_eq!(status.pending_events, 0);
    assert_eq!(status.last_event_id, Some(Uuid::nil()));
    assert_eq!(status.projection_name, "neo4j_graph");
}

#[test]
fn projection_status_reports_full_backlog_without_cursor() {
    let status = build_projection_status("neo4j_graph", None, 7, 7);

    assert!(!status.caught_up);
    assert_eq!(status.projected_events, 0);
    assert_eq!(status.pending_events, 7);
    assert_eq!(status.last_event_created_at, None);
    assert_eq!(status.last_event_id, None);
}

#[test]
fn deduplicate_graph_relations_collapses_replay_duplicates() {
    let relation = GraphRelation {
        body: Some("Shared text".to_owned()),
        direction: "incoming".to_owned(),
        entity_kind: "note".to_owned(),
        entity_ref: "note:00000000-0000-0000-0000-000000000000".to_owned(),
        relation: "XANADU_LINK".to_owned(),
        title: Some("Lease note".to_owned()),
        transclusion_id: Some(Uuid::nil()),
    };

    let deduplicated = deduplicate_graph_relations(vec![
        relation.clone(),
        relation,
        GraphRelation {
            body: Some("Dependency".to_owned()),
            direction: "outgoing".to_owned(),
            entity_kind: "task".to_owned(),
            entity_ref: "task:11111111-1111-1111-1111-111111111111".to_owned(),
            relation: "DEPENDS_ON".to_owned(),
            title: Some("Ship durable task lifecycle".to_owned()),
            transclusion_id: None,
        },
    ]);

    assert_eq!(deduplicated.len(), 2);
}

proptest::proptest! {
    #[test]
    fn projection_cursor_preserves_event_identity(event_bytes in any::<[u8; 16]>()) {
        let created_at = Utc::now();
        let event_id = Uuid::from_bytes(event_bytes);
        let cursor = ProjectionCursor::new(created_at, event_id);

        prop_assert_eq!(cursor.created_at, created_at);
        prop_assert_eq!(cursor.event_id, event_id);
    }
}

#[tokio::test]
async fn projection_coordinator_serializes_concurrent_writes() {
    let projection_coordinator = ProjectionCoordinator::default();
    let shared_barrier = Arc::new(Barrier::new(2));
    let active_writers = Arc::new(AtomicUsize::new(0));
    let peak_writers = Arc::new(AtomicUsize::new(0));

    let first = {
        let first_barrier = Arc::clone(&shared_barrier);
        let first_active_writers = Arc::clone(&active_writers);
        let first_peak_writers = Arc::clone(&peak_writers);
        let first_projection_coordinator = projection_coordinator.clone();
        tokio::spawn(async move {
            first_projection_coordinator
                .run(async move {
                    let current = first_active_writers.fetch_add(1, Ordering::SeqCst) + 1;
                    first_peak_writers.fetch_max(current, Ordering::SeqCst);
                    first_barrier.wait().await;
                    first_active_writers.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, ThreadplaneServerError>(())
                })
                .await
        })
    };

    let second = {
        let second_barrier = Arc::clone(&shared_barrier);
        let second_active_writers = Arc::clone(&active_writers);
        let second_peak_writers = Arc::clone(&peak_writers);
        let second_projection_coordinator = projection_coordinator.clone();
        tokio::spawn(async move {
            second_barrier.wait().await;
            second_projection_coordinator
                .run(async move {
                    let current = second_active_writers.fetch_add(1, Ordering::SeqCst) + 1;
                    second_peak_writers.fetch_max(current, Ordering::SeqCst);
                    second_active_writers.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, ThreadplaneServerError>(())
                })
                .await
        })
    };

    let first_result = first.await;
    assert!(first_result.is_ok(), "first projection task should join");
    let Ok(first_projection_result) = first_result else {
        return;
    };
    assert!(
        first_projection_result.is_ok(),
        "first projection should succeed"
    );

    let second_result = second.await;
    assert!(second_result.is_ok(), "second projection task should join");
    let Ok(second_projection_result) = second_result else {
        return;
    };
    assert!(
        second_projection_result.is_ok(),
        "second projection should succeed"
    );

    assert_eq!(peak_writers.load(Ordering::SeqCst), 1);
}
