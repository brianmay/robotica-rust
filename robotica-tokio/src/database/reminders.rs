//! Access the reminder silences table in the database.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// A silence that suppresses reminders with a matching key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Silence {
    /// The reminder key this silence applies to.
    pub reminder_key: String,

    /// When the silence expires, or `None` for an indefinite silence.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Load all silences that have not expired.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database
/// connection or query execution.
pub async fn load_active_silences(postgres: &PgPool) -> Result<Vec<Silence>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT reminder_key, expires_at
        FROM reminder_silences
        WHERE expires_at IS NULL OR expires_at > now()
        ORDER BY reminder_key
        "#
    )
    .fetch_all(postgres)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| Silence {
            reminder_key: row.reminder_key,
            expires_at: row.expires_at,
        })
        .collect())
}

/// Insert a silence for a reminder key.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database
/// connection or query execution.
pub async fn insert_silence(
    postgres: &PgPool,
    reminder_key: &str,
    expires_at: Option<DateTime<Utc>>,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO reminder_silences (reminder_key, expires_at, reason)
        VALUES ($1, $2, $3)
        "#,
        reminder_key,
        expires_at,
        reason
    )
    .execute(postgres)
    .await?;

    Ok(())
}

/// Delete all silences for a reminder key.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database
/// connection or query execution.
pub async fn delete_silences_for_key(
    postgres: &PgPool,
    reminder_key: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"DELETE FROM reminder_silences WHERE reminder_key = $1"#,
        reminder_key
    )
    .execute(postgres)
    .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use chrono::Duration;

    #[sqlx::test(migrations = "../migrations")]
    async fn test_load_active_silences(postgres: PgPool) {
        insert_silence(&postgres, "active", None, Some("forever"))
            .await
            .unwrap();
        insert_silence(
            &postgres,
            "future",
            Some(Utc::now() + Duration::hours(1)),
            None,
        )
        .await
        .unwrap();
        insert_silence(
            &postgres,
            "expired",
            Some(Utc::now() - Duration::hours(1)),
            None,
        )
        .await
        .unwrap();

        let silences = load_active_silences(&postgres).await.unwrap();
        let keys = silences
            .iter()
            .map(|silence| silence.reminder_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["active", "future"]);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn test_delete_silences_for_key(postgres: PgPool) {
        insert_silence(&postgres, "key", None, None).await.unwrap();
        insert_silence(&postgres, "other", None, None)
            .await
            .unwrap();

        let deleted = delete_silences_for_key(&postgres, "key").await.unwrap();
        assert_eq!(deleted, 1);

        let silences = load_active_silences(&postgres).await.unwrap();
        assert_eq!(silences.len(), 1);
        assert_eq!(silences[0].reminder_key, "other");
    }
}
