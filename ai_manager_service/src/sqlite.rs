use sqlx::{pool::PoolConnection, Sqlite};

pub async fn write_new_story_segment(
    mut conn: PoolConnection<Sqlite>,
    user_id: i64,
    event_type: String,
    story_segment: String,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
INSERT INTO story_segments ( user_id, event_type, story_segment )
VALUES ( ?, ?, ? )
        "#,
        user_id,
        event_type,
        story_segment
    )
    .execute(&mut conn)
    .await?;
    Ok(())
}

pub async fn get_story_segments_for_user(
    mut conn: PoolConnection<Sqlite>,
    user_id: i64,
) -> anyhow::Result<String> {
    let db_results = sqlx::query!(
        r#"
SELECT story_segment
FROM story_segments
WHERE user_id = ?
        "#,
        user_id,
    )
    .fetch_one(&mut conn)
    .await?;
    Ok(db_results.story_segment)
}

pub async fn get_latest_story_segments_for_user(
    mut conn: PoolConnection<Sqlite>,
    user_id: i64,
) -> anyhow::Result<String> {
    let db_results = sqlx::query!(
        r#"
SELECT story_segment
FROM story_segments
WHERE user_id = ?
        "#,
        user_id,
    )
    .fetch_one(&mut conn)
    .await?;
    Ok(db_results.story_segment)
}