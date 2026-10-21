use sqlx::SqlitePool;
use twitch_common::chat::messages::MessagePlatform;

#[derive(Debug, sqlx::FromRow)]
pub struct UserScore {
    pub id: i64,
    pub total_points: i64,
}

/// Find a player's stored score or register their first participation with zero points.
/// Database failures propagate instead of being mistaken for a missing player.
pub async fn get_player_score(
    pool: &SqlitePool,
    platform: MessagePlatform,
    user_id: &str,
    username: &str,
) -> anyhow::Result<UserScore> {
    let mut connection = pool.acquire().await?;
    let platform = platform as u8;
    if let Some(score) = sqlx::query_as::<_, UserScore>(
        "SELECT id, total_points FROM wordle_highscores WHERE platform = ? AND platform_user_id = ?",
    )
    .bind(platform)
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await?
    {
        return Ok(score);
    }

    Ok(sqlx::query_as::<_, UserScore>(
        "INSERT INTO wordle_highscores (username, platform_user_id, platform, total_points)
         VALUES (?, ?, ?, 0) RETURNING id, total_points",
    )
    .bind(username)
    .bind(user_id)
    .bind(platform)
    .fetch_one(&mut *connection)
    .await?)
}

/// One row of the on-screen leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LeaderboardEntry {
    pub username: String,
    pub total_points: i64,
}

/// The highest scoring players across every platform, ties broken by name.
pub async fn top_scores(pool: &SqlitePool, limit: u32) -> anyhow::Result<Vec<LeaderboardEntry>> {
    Ok(sqlx::query_as::<_, LeaderboardEntry>(
        "SELECT username, total_points FROM wordle_highscores
         ORDER BY total_points DESC, username ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Add `points` to a player's running total, registering them first if needed.
/// The stored username is refreshed so renamed players keep their history.
pub async fn add_points(
    pool: &SqlitePool,
    platform: MessagePlatform,
    user_id: &str,
    username: &str,
    points: i64,
) -> anyhow::Result<UserScore> {
    let score = get_player_score(pool, platform, user_id, username).await?;
    Ok(sqlx::query_as::<_, UserScore>(
        "UPDATE wordle_highscores SET total_points = total_points + ?, username = ?
         WHERE id = ? RETURNING id, total_points",
    )
    .bind(points)
    .bind(username)
    .bind(score.id)
    .fetch_one(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        sql_common::setup::setup_sqlite_migrations("sqlite::memory:".into(), &crate::MIGRATIONS)
            .await
            .expect("in-memory scoreboard")
    }

    #[tokio::test]
    async fn scoreboard_uses_only_game_migrations() {
        scoreboard_uses_only_game_migrations_result()
            .await
            .expect("test failed");
    }

    async fn scoreboard_uses_only_game_migrations_result() -> anyhow::Result<()> {
        let pool = test_pool().await;
        crate::MIGRATIONS.run(&pool).await?;
        let first = get_player_score(&pool, MessagePlatform::Twitch, "player-1", "Player").await?;
        let returning =
            get_player_score(&pool, MessagePlatform::Twitch, "player-1", "Player").await?;
        assert_eq!(first.id, returning.id);
        assert_eq!(first.total_points, 0);
        assert_eq!(first.total_points, returning.total_points);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wordle_highscores")
            .fetch_one(&pool)
            .await?;
        assert_eq!(count, 1);
        let alerts_tables: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'raid_events'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(alerts_tables, 0);
        Ok(())
    }

    #[tokio::test]
    async fn lookup_errors_do_not_insert_a_player() {
        let pool = test_pool().await;
        sqlx::query("INSERT INTO wordle_highscores (username, platform_user_id, platform, total_points) VALUES ('Player', 'player-1', 0, 'invalid score')")
            .execute(&pool).await.unwrap();
        let result = get_player_score(&pool, MessagePlatform::Twitch, "player-1", "Player").await;
        assert!(result.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wordle_highscores")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn top_scores_orders_by_points_then_name() {
        let pool = test_pool().await;
        add_points(&pool, MessagePlatform::Twitch, "1", "Bravo", 10)
            .await
            .unwrap();
        add_points(&pool, MessagePlatform::Twitch, "2", "Alpha", 10)
            .await
            .unwrap();
        add_points(&pool, MessagePlatform::Youtube, "3", "Charlie", 50)
            .await
            .unwrap();
        add_points(&pool, MessagePlatform::Twitch, "4", "Delta", 1)
            .await
            .unwrap();
        let names: Vec<String> = top_scores(&pool, 3)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| format!("{}:{}", entry.username, entry.total_points))
            .collect();
        assert_eq!(names, ["Charlie:50", "Alpha:10", "Bravo:10"]);
        assert!(top_scores(&test_pool().await, 3).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn points_accumulate_per_player_and_platform() {
        let pool = test_pool().await;
        let first = add_points(&pool, MessagePlatform::Twitch, "player-1", "Player", 7)
            .await
            .unwrap();
        assert_eq!(first.total_points, 7);
        let second = add_points(&pool, MessagePlatform::Twitch, "player-1", "Renamed", 35)
            .await
            .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.total_points, 42);
        let other = add_points(&pool, MessagePlatform::Youtube, "player-1", "Player", 3)
            .await
            .unwrap();
        assert_ne!(other.id, first.id);
        assert_eq!(other.total_points, 3);
        let username: String =
            sqlx::query_scalar("SELECT username FROM wordle_highscores WHERE id = ?")
                .bind(first.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(username, "Renamed");
    }
}
