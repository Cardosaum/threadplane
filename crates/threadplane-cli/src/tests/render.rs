use super::*;

#[test]
fn render_task_list_compact_formats_ready_tasks() {
    let task_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default();
    let rendered = render_task_list_compact(&[TaskListEntry {
        active_claim: Some(TaskClaimRecord {
            actor: "codex".to_owned(),
            claim_id: Uuid::parse_str("aaaaaaaa-2222-3333-4444-555555555555").unwrap_or_default(),
            claimed_at: "2026-03-17T04:25:00Z".to_owned(),
            event_id: Uuid::parse_str("bbbbbbbb-2222-3333-4444-555555555555").unwrap_or_default(),
            expires_at: "2026-03-17T04:30:00Z".to_owned(),
            task_id,
            workspace: "threadplane-dev".to_owned(),
        }),
        dependencies: vec![TaskDependencySummary {
            depth: 1,
            entity_ref: "task:aaaaaaaa-0000-0000-0000-000000000000".to_owned(),
            status: "completed".to_owned(),
            task_id: Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap_or_default(),
            title: "Ship durable task lifecycle".to_owned(),
        }],
        dependents: Vec::new(),
        epic: Some(EpicRecord {
            author: "operator".to_owned(),
            body: "Dogfood the repo itself.".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            entity_ref: "epic:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
            epic_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            event_id: Uuid::parse_str("cccccccc-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            title: "Dogfooding".to_owned(),
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        }),
        ready: true,
        task: TaskSummary {
            author: "codex".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            details: "Keep the queue readable.".to_owned(),
            entity_ref: "task:11111111-2222-3333-4444-555555555555".to_owned(),
            epic_id: Some(
                Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            ),
            metadata: sample_task_metadata(),
            status: "open".to_owned(),
            task_id,
            title: "Add concise ready-queue views".to_owned(),
            transclusion_id: None,
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        },
    }]);

    assert!(rendered.contains("11111111 | Add concise ready-queue views"));
    assert!(rendered.contains("status=open"));
    assert!(rendered.contains("priority=high"));
    assert!(rendered.contains("ready"));
    assert!(rendered.contains("deps=1"));
    assert!(rendered.contains("epic=Dogfooding"));
    assert!(rendered.contains("owner=codex"));
    assert!(rendered.contains("labels=workflow,agent"));
    assert!(rendered.contains("claim=codex"));
}

#[test]
fn render_task_list_compact_handles_empty_lists() {
    assert_eq!(render_task_list_compact(&[]), "no tasks\n");
}

#[test]
fn render_task_dependency_compact_formats_entries() {
    let rendered = render_task_dependency_compact(&[TaskDependencySummary {
        depth: 2,
        entity_ref: "task:aaaaaaaa-0000-0000-0000-000000000000".to_owned(),
        status: "completed".to_owned(),
        task_id: Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap_or_default(),
        title: "Ship durable task lifecycle".to_owned(),
    }]);

    assert!(rendered.contains("aaaaaaaa | Ship durable task lifecycle"));
    assert!(rendered.contains("status=completed"));
    assert!(rendered.contains("depth=2"));
}

#[test]
fn render_task_dependency_compact_handles_empty_lists() {
    assert_eq!(render_task_dependency_compact(&[]), "no tasks\n");
}

#[test]
fn render_note_list_compact_formats_entries() {
    let rendered = render_note_list_compact(&[NoteRecord {
        author: "codex".to_owned(),
        body: "Lease notes".to_owned(),
        created_at: "2026-03-17T04:00:00Z".to_owned(),
        entity_ref: "note:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
        event_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
        note_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default(),
        title: "Lease design".to_owned(),
        transclusion_id: None,
        updated_at: "2026-03-17T04:05:00Z".to_owned(),
        workspace: "threadplane-dev".to_owned(),
    }]);

    assert!(rendered.contains("11111111 | Lease design | author=codex"));
    assert!(rendered.contains("updated_at=2026-03-17T04:05:00Z"));
}

#[test]
fn render_memory_list_compact_formats_entries() {
    let rendered = render_memory_list_compact(&[MemoryRecord {
        audience: MemoryAudience::Agent,
        author: "codex".to_owned(),
        body: "Start with clean bottom-up abstractions.".to_owned(),
        created_at: "2026-03-17T04:00:00Z".to_owned(),
        entity_ref: "memory:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
        event_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
        importance: MemoryImportance::Critical,
        kind: MemoryKind::from_lossy("workflow"),
        memory_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default(),
        recall_triggers: vec!["session_start".to_owned()],
        scope: MemoryScope::Workspace,
        tags: vec!["core".to_owned(), "prime".to_owned()],
        title: "Implementation quality first".to_owned(),
        updated_at: "2026-03-17T04:05:00Z".to_owned(),
        workspace: "threadplane-dev".to_owned(),
    }]);

    assert!(rendered.contains("11111111 | Implementation quality first"));
    assert!(rendered.contains("kind=workflow"));
    assert!(rendered.contains("importance=critical"));
    assert!(rendered.contains("audience=agent"));
    assert!(rendered.contains("tags=core,prime"));
}

#[test]
fn render_event_list_compact_formats_entries() {
    let rendered = render_event_list_compact(&[EventRecord {
        actor: "codex".to_owned(),
        created_at: "2026-03-17T04:05:00Z".to_owned(),
        event_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
        kind: EventKind::TaskOffered,
        payload: serde_json::json!({"task_id": "11111111-2222-3333-4444-555555555555"}),
        workspace: "threadplane-dev".to_owned(),
    }]);

    assert!(rendered.contains("aaaaaaaa | task_offered | actor=codex"));
    assert!(rendered.contains("at=2026-03-17T04:05:00Z"));
}

#[test]
fn render_graph_relations_compact_formats_entries() {
    let rendered = render_graph_relations_compact(&[GraphRelation {
        body: Some("Shared note".to_owned()),
        direction: "incoming".to_owned(),
        entity_kind: "note".to_owned(),
        entity_ref: "note:11111111-2222-3333-4444-555555555555".to_owned(),
        relation: "XANADU_LINK".to_owned(),
        title: Some("Lease wording".to_owned()),
        transclusion_id: None,
    }]);

    assert!(rendered.contains("incoming XANADU_LINK"));
    assert!(rendered.contains("note:11111111"));
    assert!(rendered.contains("Lease wording"));
}

#[test]
fn render_entity_context_compact_formats_task_summary_and_relations() {
    let rendered = render_entity_context_compact(&EntityContext {
        entity: EntityRecord::Epic(EpicRecord {
            author: "operator".to_owned(),
            body: "Dogfood the repo itself.".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            entity_ref: "epic:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
            epic_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            event_id: Uuid::parse_str("cccccccc-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            title: "Dogfooding".to_owned(),
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        }),
        relations: vec![GraphRelation {
            body: Some("Shared note".to_owned()),
            direction: "outgoing".to_owned(),
            entity_kind: "task".to_owned(),
            entity_ref: "task:11111111-2222-3333-4444-555555555555".to_owned(),
            relation: "IMPLEMENTS_EPIC".to_owned(),
            title: Some("Ship durable task lifecycle".to_owned()),
            transclusion_id: None,
        }],
    });

    assert!(rendered.contains("epic aaaaaaaa | Dogfooding"));
    assert!(rendered.contains("outgoing IMPLEMENTS_EPIC"));
    assert!(rendered.contains("task:11111111"));
}

#[test]
fn render_entity_context_compact_formats_memory_summary() {
    let rendered = render_entity_context_compact(&EntityContext {
        entity: EntityRecord::Memory(MemoryRecord {
            audience: MemoryAudience::Both,
            author: "operator".to_owned(),
            body: "Always prime agents with startup context.".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            entity_ref: "memory:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
            event_id: Uuid::parse_str("cccccccc-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            importance: MemoryImportance::High,
            kind: MemoryKind::from_lossy("workflow"),
            memory_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            recall_triggers: vec!["session_start".to_owned()],
            scope: MemoryScope::Workspace,
            tags: vec!["prime".to_owned()],
            title: "Prime before coding".to_owned(),
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        }),
        relations: Vec::new(),
    });

    assert!(rendered.contains("memory aaaaaaaa | Prime before coding"));
    assert!(rendered.contains("kind=workflow"));
    assert!(rendered.contains("importance=high"));
}
