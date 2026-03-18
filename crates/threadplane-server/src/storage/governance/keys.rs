use super::*;

#[derive(Debug, FromRow)]
struct ActorPublicKeyRow {
    actor_id: String,
    algorithm: String,
    key_id: String,
    public_key: String,
}

pub(crate) async fn fetch_actor_public_keys(
    pool: &PgPool,
    workspace: &str,
    actor_id: Option<&str>,
) -> ServerResult<Vec<ActorPublicKey>> {
    let rows = if let Some(selected_actor_id) = actor_id {
        query_as::<_, ActorPublicKeyRow>(
            "
            SELECT actor_id, algorithm, key_id, public_key
            FROM actor_public_keys
            WHERE workspace = $1
              AND actor_id = $2
            ORDER BY actor_id ASC, key_id ASC
            ",
        )
        .bind(workspace)
        .bind(selected_actor_id)
        .fetch_all(pool)
        .await?
    } else {
        query_as::<_, ActorPublicKeyRow>(
            "
            SELECT actor_id, algorithm, key_id, public_key
            FROM actor_public_keys
            WHERE workspace = $1
            ORDER BY actor_id ASC, key_id ASC
            ",
        )
        .bind(workspace)
        .fetch_all(pool)
        .await?
    };

    rows.into_iter().map(TryInto::try_into).collect()
}

pub(crate) async fn upsert_actor_public_key(
    pool: &PgPool,
    workspace: &str,
    key: &ActorPublicKey,
) -> ServerResult<ActorPublicKey> {
    sqlx::query(
        "
        INSERT INTO actor_public_keys (
            workspace,
            actor_id,
            key_id,
            algorithm,
            public_key,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (workspace, actor_id, key_id) DO UPDATE
        SET algorithm = excluded.algorithm,
            public_key = excluded.public_key,
            updated_at = now()
        ",
    )
    .bind(workspace)
    .bind(&key.actor_id)
    .bind(&key.key_id)
    .bind(key.algorithm.to_string())
    .bind(&key.public_key)
    .execute(pool)
    .await?;
    Ok(key.clone())
}

pub(super) fn parse_public_key_algorithms(
    values: &[String],
) -> ServerResult<Vec<PublicKeyAlgorithm>> {
    values
        .iter()
        .map(|value| parse_public_key_algorithm(value))
        .collect()
}

pub(super) fn parse_public_key_algorithm(value: &str) -> ServerResult<PublicKeyAlgorithm> {
    value.parse().map_err(|_error| {
        ThreadplaneServerError::internal(format!("unsupported stored public-key algorithm {value}"))
    })
}

pub(super) fn serialize_public_key_algorithm(value: PublicKeyAlgorithm) -> String {
    match value {
        PublicKeyAlgorithm::Ed25519 => "ed25519".to_owned(),
        PublicKeyAlgorithm::Secp256k1 => "secp256k1".to_owned(),
        PublicKeyAlgorithm::SshEd25519 => "ssh_ed25519".to_owned(),
    }
}

impl TryFrom<ActorPublicKeyRow> for ActorPublicKey {
    type Error = ThreadplaneServerError;

    #[inline]
    fn try_from(value: ActorPublicKeyRow) -> ServerResult<Self> {
        Ok(Self {
            actor_id: value.actor_id,
            algorithm: parse_public_key_algorithm(&value.algorithm)?,
            key_id: value.key_id,
            public_key: value.public_key,
        })
    }
}
