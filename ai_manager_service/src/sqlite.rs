use anyhow::Ok;
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
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn write_new_gift_subs_event(
    mut conn: PoolConnection<Sqlite>,
    event: &messages::ChannelGiftMessage,
    tier: String,
    story_segment: String
) -> anyhow::Result<()> {

    sqlx::query!(
        r#"
INSERT INTO gift_subs_events (
    broadcaster_user_id, cumulative_total, is_anonymous,
    tier, total, user_id, user_login, user_name, story_segment 
)
VALUES (?, ?, ?, ?, ?, ?, ? ,? ,?)
    "#,
    event.broadcaster_user_id, event.cumulative_total,
    event.is_anonymous, tier, event.total, event.user_id,
    event.user_login, event.user_name, story_segment
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

pub async fn write_new_raid_event(
    mut conn: PoolConnection<Sqlite>,
    event: &messages::RaidEvent,
    story_segment: String
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
INSERT INTO raid_events ( from_broadcaster_user_id,
     from_broadcaster_user_name, to_broadcaster_user_id, to_broadcaster_user_name,
     viewers, story_segment)
VALUES ( ?, ?, ?, ?, ?, ? )
        "#, 
        event.from_broadcaster_user_id,
        event.from_broadcaster_user_name,
        event.to_broadcaster_user_id,
        event.to_broadcaster_user_name,
        event.viewers,
        story_segment,
    )
    .execute(&mut *conn)
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
    .fetch_one(&mut *conn)
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
    .fetch_one(&mut *conn)
    .await?;
    Ok(db_results.story_segment)
}
