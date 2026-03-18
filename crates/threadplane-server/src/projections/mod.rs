#![expect(
    clippy::redundant_pub_crate,
    reason = "Projection helpers are crate-local adapters around Neo4j."
)]

mod reads;
mod transclusion;
mod writes;

#[cfg(test)]
pub(crate) use reads::deduplicate_graph_relations;
pub(crate) use reads::fetch_entity_relations;
pub(crate) use transclusion::reproject_transclusion_group;
pub(crate) use writes::{
    project_claim, project_epic, project_link, project_memory, project_note, project_task,
    project_task_dependency_by_id, project_task_supporting_entities,
};
