use crate::messages;
use sqlx::{pool::PoolConnection, Sqlite};

pub async fn write_new_story_segment(
    mut conn: PoolConnection<Sqlite>,
    user_id: i64,
    event_type: String,
    story_segment: String,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO story_segments (user_id, event_type, story_segment) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(event_type)
        .bind(story_segment)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn write_new_gift_subs_event(
    mut conn: PoolConnection<Sqlite>,
    event: &messages::ChannelGiftMessage,
    tier: String,
    story_segment: String,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO gift_subs_events (
            broadcaster_user_id, cumulative_total, is_anonymous,
            tier, total, user_id, user_login, user_name, story_segment
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event.broadcaster_user_id)
    .bind(event.cumulative_total)
    .bind(event.is_anonymous)
    .bind(tier)
    .bind(event.total)
    .bind(&event.user_id)
    .bind(&event.user_login)
    .bind(&event.user_name)
    .bind(story_segment)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn write_new_raid_event(
    mut conn: PoolConnection<Sqlite>,
    event: &messages::RaidEvent,
    story_segment: String,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO raid_events (
            from_broadcaster_user_id, from_broadcaster_user_name,
            to_broadcaster_user_id, to_broadcaster_user_name, viewers, story_segment
        ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&event.from_broadcaster_user_id)
    .bind(&event.from_broadcaster_user_name)
    .bind(&event.to_broadcaster_user_id)
    .bind(&event.to_broadcaster_user_name)
    .bind(event.viewers)
    .bind(story_segment)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn raid_storage_uses_only_alert_migrations() {
        raid_storage_uses_only_alert_migrations_result()
            .await
            .expect("test failed");
    }

    async fn raid_storage_uses_only_alert_migrations_result() -> anyhow::Result<()> {
        let pool = sql_common::setup::setup_sqlite_migrations(
            "sqlite::memory:".into(),
            &crate::MIGRATIONS,
        )
        .await?;
        crate::MIGRATIONS.run(&pool).await?;
        let event = messages::RaidEvent {
            from_broadcaster_user_id: "123".into(),
            from_broadcaster_user_login: "raider".into(),
            from_broadcaster_user_name: "Raider".into(),
            to_broadcaster_user_id: "456".into(),
            to_broadcaster_user_login: "host".into(),
            to_broadcaster_user_name: "Host".into(),
            viewers: 42,
        };
        write_new_raid_event(pool.acquire().await?, &event, "A raid arrives".into()).await?;
        let stored: (String, i64, String) = sqlx::query_as(
            "SELECT from_broadcaster_user_id, viewers, story_segment FROM raid_events",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(stored, ("123".into(), 42, "A raid arrives".into()));
        let game_tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'wordle_highscores'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(game_tables, 0);
        Ok(())
    }

    #[tokio::test]
    async fn story_segments_are_persisted() {
        let pool = sql_common::setup::setup_sqlite_migrations(
            "sqlite::memory:".into(),
            &crate::MIGRATIONS,
        )
        .await
        .unwrap();
        write_new_story_segment(
            pool.acquire().await.unwrap(),
            42,
            "follow".into(),
            "The party grows".into(),
        )
        .await
        .unwrap();
        let row: (i64, String, String) =
            sqlx::query_as("SELECT user_id, event_type, story_segment FROM story_segments")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row, (42, "follow".into(), "The party grows".into()));
    }

    #[tokio::test]
    async fn anonymous_and_identified_gifts_are_persisted() {
        let pool = sql_common::setup::setup_sqlite_migrations(
            "sqlite::memory:".into(),
            &crate::MIGRATIONS,
        )
        .await
        .unwrap();
        let mut event = messages::ChannelGiftMessage {
            broadcaster_user_id: "456".into(),
            broadcaster_user_login: "host".into(),
            broadcaster_user_name: "Host".into(),
            cumulative_total: None,
            is_anonymous: true,
            tier: messages::NullSubTier::Tier1("rare".into()),
            total: 5,
            user_id: None,
            user_login: None,
            user_name: None,
        };
        write_new_gift_subs_event(
            pool.acquire().await.unwrap(),
            &event,
            "rare".into(),
            "Anonymous gift".into(),
        )
        .await
        .unwrap();
        event.is_anonymous = false;
        event.cumulative_total = Some(12);
        event.user_id = Some("123".into());
        event.user_login = Some("gifter".into());
        event.user_name = Some("Gifter".into());
        write_new_gift_subs_event(
            pool.acquire().await.unwrap(),
            &event,
            "rare".into(),
            "Named gift".into(),
        )
        .await
        .unwrap();
        #[derive(Debug, PartialEq, sqlx::FromRow)]
        struct Gift {
            user_id: Option<String>,
            cumulative_total: Option<i64>,
            is_anonymous: bool,
            story_segment: String,
        }
        let gifts: Vec<Gift> = sqlx::query_as(
            "SELECT user_id, cumulative_total, is_anonymous, story_segment FROM gift_subs_events ORDER BY id"
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(
            gifts,
            vec![
                Gift {
                    user_id: None,
                    cumulative_total: None,
                    is_anonymous: true,
                    story_segment: "Anonymous gift".into()
                },
                Gift {
                    user_id: Some("123".into()),
                    cumulative_total: Some(12),
                    is_anonymous: false,
                    story_segment: "Named gift".into()
                },
            ]
        );
    }

    #[tokio::test]
    async fn gift_migration_preserves_existing_records() {
        let previous = sqlx::migrate::Migrator::with_migrations(
            crate::MIGRATIONS
                .iter()
                .filter(|migration| migration.version < 20260911000000)
                .cloned()
                .collect(),
        );
        let pool = sql_common::setup::setup_sqlite_migrations("sqlite::memory:".into(), &previous)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO gift_subs_events (id, broadcaster_user_id, cumulative_total, is_anonymous, tier, total, user_id, user_login, user_name, story_segment)
             VALUES (7, 'host-id', 20, 0, 1000, 5, 'gifter-id', 'gifter', 'Gifter', 'Existing story')"
        ).execute(&pool).await.unwrap();
        crate::MIGRATIONS.run(&pool).await.unwrap();
        crate::MIGRATIONS.run(&pool).await.unwrap();
        let row: (i64, String, String, i64, String) = sqlx::query_as(
            "SELECT id, user_id, tier, cumulative_total, story_segment FROM gift_subs_events",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                7,
                "gifter-id".into(),
                "1000".into(),
                20,
                "Existing story".into()
            )
        );
    }
}
