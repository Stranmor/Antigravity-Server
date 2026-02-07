use sqlx::PgPool;

pub async fn store_signature(
    pool: &PgPool,
    content_hash: &str,
    signature: &str,
    model_family: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO thinking_signatures (content_hash, signature, model_family, last_used_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (content_hash) DO UPDATE SET
            signature = EXCLUDED.signature,
            model_family = EXCLUDED.model_family,
            last_used_at = NOW()
        "#,
    )
    .bind(content_hash)
    .bind(signature)
    .bind(model_family)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_signature(
    pool: &PgPool,
    content_hash: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let result: Option<(String, String)> = sqlx::query_as(
        r#"
        UPDATE thinking_signatures
        SET last_used_at = NOW()
        WHERE content_hash = $1
        RETURNING signature, model_family
        "#,
    )
    .bind(content_hash)
    .fetch_optional(pool)
    .await?;
    Ok(result)
}

pub async fn cleanup_old_signatures(pool: &PgPool, days: i32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM thinking_signatures
        WHERE last_used_at < NOW() - make_interval(days => $1)
        "#,
    )
    .bind(days)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// ===== Session Signatures (persistent session → signature mapping) =====

pub async fn store_session_signature(
    pool: &PgPool,
    session_id: &str,
    signature: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO session_signatures (session_id, signature, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (session_id) DO UPDATE SET
            signature = EXCLUDED.signature,
            updated_at = NOW()
        "#,
    )
    .bind(session_id)
    .bind(signature)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_session_signature(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let result: Option<(String,)> = sqlx::query_as(
        r#"
        UPDATE session_signatures
        SET updated_at = NOW()
        WHERE session_id = $1
        RETURNING signature
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(result.map(|(sig,)| sig))
}

pub async fn cleanup_old_session_signatures(pool: &PgPool, days: i32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM session_signatures
        WHERE updated_at < NOW() - make_interval(days => $1)
        "#,
    )
    .bind(days)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
