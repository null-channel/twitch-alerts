use sqlx::{pool::PoolConnection, Sqlite};


pub async fn write_new_story_segment(mut conn: PoolConnection<Sqlite>, user_id: i64, event_type: String, story_segment: String ) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
INSERT INTO story_segments ( user_id, event_type, story_segment )
VALUES ( ?, ?, ? )
        "#,
        user_id, event_type, story_segment
    )
    .execute(&mut conn)
    .await?;
    Ok(())
}