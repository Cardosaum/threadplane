#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::super::*;

pub(crate) async fn show_entity(
    State(graph): State<Arc<Graph>>,
    State(pool): State<PgPool>,
    Path(EntityPath { entity_ref }): Path<EntityPath>,
) -> AppResult<EntityContext> {
    let entity = fetch_entity_record(&pool, &entity_ref).await?;
    let relations = fetch_entity_relations(graph.as_ref(), &entity_ref)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success(EntityContext { entity, relations }))
}

pub(crate) async fn related_entities(
    State(graph): State<Arc<Graph>>,
    State(pool): State<PgPool>,
    Path(EntityPath { entity_ref }): Path<EntityPath>,
) -> AppResult<Vec<threadplane_core::GraphRelation>> {
    let _entity = fetch_entity_record(&pool, &entity_ref).await?;
    let relations = fetch_entity_relations(graph.as_ref(), &entity_ref)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success(relations))
}
