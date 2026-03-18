use neo4rs::query;

use crate::prelude::*;
use threadplane_core::GraphRelation;

pub(crate) async fn fetch_entity_relations(
    graph: &Graph,
    entity_ref: &str,
) -> ServerResult<Vec<GraphRelation>> {
    let mut result = graph
        .execute(
            query(
                "
                MATCH (entity:Entity {entity_ref: $entity_ref})
                OPTIONAL MATCH (entity)-[rel]-(other:Entity)
                RETURN
                  DISTINCT type(rel) AS relation,
                  CASE
                    WHEN rel IS NULL THEN NULL
                    WHEN startNode(rel).entity_ref = $entity_ref THEN 'outgoing'
                    ELSE 'incoming'
                  END AS direction,
                  other.entity_ref AS entity_ref,
                  coalesce(other.kind, 'unknown') AS entity_kind,
                  other.title AS title,
                  coalesce(other.body, other.details) AS body,
                  NULLIF(other.transclusion_id, '') AS transclusion_id
                ORDER BY relation, entity_ref
                ",
            )
            .param("entity_ref", entity_ref.to_owned()),
        )
        .await?;

    let mut relations = Vec::new();
    loop {
        let maybe_row = result.next().await?;
        let Some(row) = maybe_row else {
            break;
        };

        let relation_opt: Option<String> = row.get("relation")?;
        let related_entity_ref_opt: Option<String> = row.get("entity_ref")?;
        let entity_kind_opt: Option<String> = row.get("entity_kind")?;
        let direction_opt: Option<String> = row.get("direction")?;
        let title: Option<String> = row.get("title")?;
        let body: Option<String> = row.get("body")?;
        let transclusion_id: Option<String> = row.get("transclusion_id")?;

        if let (Some(relation), Some(related_entity_ref), Some(entity_kind), Some(direction)) = (
            relation_opt,
            related_entity_ref_opt,
            entity_kind_opt,
            direction_opt,
        ) {
            relations.push(GraphRelation {
                relation,
                direction,
                entity_ref: related_entity_ref,
                entity_kind,
                title,
                body,
                transclusion_id: transclusion_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
            });
        }
    }

    Ok(deduplicate_graph_relations(relations))
}

pub(crate) fn deduplicate_graph_relations(mut relations: Vec<GraphRelation>) -> Vec<GraphRelation> {
    relations.sort_unstable();
    relations.dedup();
    relations
}
