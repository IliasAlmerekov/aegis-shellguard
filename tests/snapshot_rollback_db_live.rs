//! Live database snapshot/rollback integration tests for [`PostgresPlugin`],
//! [`MysqlPlugin`], and [`SupabasePlugin`].
//!
//! Each test is gated behind its own opt-in environment variable so default
//! `cargo test` requires no running database server:
//! - `AEGIS_POSTGRES_SNAPSHOT_TESTS=1` (against PostgreSQL 16)
//! - `AEGIS_MYSQL_SNAPSHOT_TESTS=1` (against MySQL 8.0, image `mysql:8.0`)
//! - `AEGIS_SUPABASE_SNAPSHOT_TESTS=1` (against PostgreSQL 16 via Supabase's
//!   Postgres-compatible dump/restore path)

use std::process::Command;

use aegis_config::{PostgresSnapshotConfig, SupabaseSnapshotConfig};
use aegis_snapshot::{MysqlPlugin, PostgresPlugin, SnapshotPlugin, SupabasePlugin};
use tempfile::TempDir;

struct PgConn {
    host: String,
    port: u16,
    user: String,
    database: String,
}

impl PgConn {
    fn from_env(db_env: &str, default_db: &str) -> Self {
        let host = std::env::var("PGHOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("PGPORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(5432);
        let user = std::env::var("PGUSER").unwrap_or_else(|_| "postgres".to_string());
        let database = std::env::var(db_env).unwrap_or_else(|_| default_db.to_string());
        assert!(
            !database.is_empty()
                && database != "postgres"
                && database
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "test database name must be a non-empty identifier other than 'postgres': {database:?}"
        );
        Self {
            host,
            port,
            user,
            database,
        }
    }

    fn run_psql(&self, database: &str, sql: &str) -> String {
        let output = Command::new("psql")
            .args([
                "-h",
                &self.host,
                "-p",
                &self.port.to_string(),
                "-U",
                &self.user,
                "-d",
                database,
                "-X",
                "-v",
                "ON_ERROR_STOP=1",
                "-t",
                "-A",
                "-c",
                sql,
            ])
            .output()
            .unwrap_or_else(|error| panic!("psql should run: {error}"));

        assert!(
            output.status.success(),
            "psql failed for `{sql}`\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn recreate_database(&self) {
        self.run_psql(
            "postgres",
            &format!("DROP DATABASE IF EXISTS {};", self.database),
        );
        self.run_psql("postgres", &format!("CREATE DATABASE {};", self.database));
    }
}

struct PgDbGuard<'a>(&'a PgConn);

impl Drop for PgDbGuard<'_> {
    fn drop(&mut self) {
        // Best-effort cleanup of the disposable test database.
        let _ = Command::new("psql")
            .args([
                "-h",
                &self.0.host,
                "-p",
                &self.0.port.to_string(),
                "-U",
                &self.0.user,
                "-d",
                "postgres",
                "-X",
                "-c",
                &format!("DROP DATABASE IF EXISTS {};", self.0.database),
            ])
            .output();
    }
}

struct MysqlConn {
    host: String,
    port: u16,
    user: String,
    database: String,
}

impl MysqlConn {
    fn from_env() -> Self {
        let host = std::env::var("MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("MYSQL_TCP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3306);
        let user = std::env::var("MYSQL_USER").unwrap_or_else(|_| "root".to_string());
        let database =
            std::env::var("AEGIS_MYSQL_TEST_DB").unwrap_or_else(|_| "aegis_mysql_test".to_string());
        assert!(
            !database.is_empty()
                && database
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "test database name must be a non-empty identifier: {database:?}"
        );
        Self {
            host,
            port,
            user,
            database,
        }
    }

    fn run_mysql(&self, database: Option<&str>, sql: &str) -> String {
        let mut command = Command::new("mysql");
        command.args([
            &format!("--host={}", self.host),
            &format!("--port={}", self.port),
            &format!("--user={}", self.user),
            "--batch",
            "--skip-column-names",
        ]);
        if let Some(db) = database {
            command.arg(db);
        }
        let output = command
            .args(["-e", sql])
            .output()
            .unwrap_or_else(|error| panic!("mysql should run: {error}"));

        assert!(
            output.status.success(),
            "mysql failed for `{sql}`\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn recreate_database(&self) {
        self.run_mysql(
            None,
            &format!(
                "DROP DATABASE IF EXISTS `{db}`; CREATE DATABASE `{db}`;",
                db = self.database
            ),
        );
    }
}

struct MysqlDbGuard<'a>(&'a MysqlConn);

impl Drop for MysqlDbGuard<'_> {
    fn drop(&mut self) {
        // Best-effort cleanup of the disposable test database.
        let _ = Command::new("mysql")
            .args([
                &format!("--host={}", self.0.host),
                &format!("--port={}", self.0.port),
                &format!("--user={}", self.0.user),
                "-e",
                &format!("DROP DATABASE IF EXISTS `{}`;", self.0.database),
            ])
            .output();
    }
}

fn live_test_enabled(env_var: &str, requirement: &str) -> bool {
    if std::env::var(env_var).is_ok() {
        true
    } else {
        eprintln!("skipping: set {env_var}=1 with {requirement} to run this test");
        false
    }
}

async fn run_pg_rollback_cycle(conn: &PgConn, plugin: &dyn SnapshotPlugin, label: &str) {
    let _guard = PgDbGuard(conn);
    conn.recreate_database();
    conn.run_psql(
        &conn.database,
        "CREATE TABLE items (id SERIAL PRIMARY KEY, name TEXT NOT NULL); \
         INSERT INTO items (name) VALUES ('before');",
    );
    assert_eq!(
        conn.run_psql(
            &conn.database,
            "SELECT string_agg(name, ',' ORDER BY id) FROM items;"
        ),
        "before",
        "{label}: initial seed row must be present before snapshot"
    );

    let cwd = TempDir::new().unwrap();
    assert!(
        plugin.is_applicable(cwd.path()).await,
        "{label}: plugin must be applicable when PostgreSQL client tools are on PATH"
    );

    let snapshot_id = plugin
        .snapshot(cwd.path(), "DROP TABLE items")
        .await
        .unwrap_or_else(|err| panic!("{label}: snapshot must succeed against live server: {err}"));

    conn.run_psql(&conn.database, "INSERT INTO items (name) VALUES ('after');");
    assert_eq!(
        conn.run_psql(
            &conn.database,
            "SELECT string_agg(name, ',' ORDER BY id) FROM items;"
        ),
        "before,after",
        "{label}: mutation must be visible before rollback"
    );

    plugin
        .rollback(&snapshot_id)
        .await
        .unwrap_or_else(|err| panic!("{label}: rollback must succeed against live server: {err}"));

    assert_eq!(
        conn.run_psql(
            &conn.database,
            "SELECT string_agg(name, ',' ORDER BY id) FROM items;"
        ),
        "before",
        "{label}: rollback must restore the pre-mutation database contents"
    );

    plugin
        .delete(&snapshot_id)
        .await
        .unwrap_or_else(|err| panic!("{label}: snapshot delete must succeed: {err}"));
}

#[tokio::test]
async fn postgres_snapshot_rollback_restores_live_database() {
    if live_test_enabled(
        "AEGIS_POSTGRES_SNAPSHOT_TESTS",
        "a running PostgreSQL server",
    ) {
        let conn = PgConn::from_env("AEGIS_POSTGRES_TEST_DB", "aegis_pg_test");
        let temp = TempDir::new().unwrap();
        let plugin = PostgresPlugin::new(
            conn.database.clone(),
            conn.host.clone(),
            conn.port,
            conn.user.clone(),
            temp.path().join("snapshots"),
        );
        run_pg_rollback_cycle(&conn, &plugin, "postgres").await;
    }
}

#[tokio::test]
async fn mysql_snapshot_rollback_restores_live_database() {
    if live_test_enabled("AEGIS_MYSQL_SNAPSHOT_TESTS", "a running MySQL 8.0 server") {
        let conn = MysqlConn::from_env();
        let _guard = MysqlDbGuard(&conn);
        conn.recreate_database();
        conn.run_mysql(
            Some(&conn.database),
            "CREATE TABLE items (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(64) NOT NULL); \
             INSERT INTO items (name) VALUES ('before');",
        );
        assert_eq!(
            conn.run_mysql(
                Some(&conn.database),
                "SELECT GROUP_CONCAT(name ORDER BY id SEPARATOR ',') FROM items;"
            ),
            "before",
            "initial seed row must be present before snapshot"
        );

        let temp = TempDir::new().unwrap();
        let plugin = MysqlPlugin::new(
            conn.database.clone(),
            conn.host.clone(),
            conn.port,
            conn.user.clone(),
            temp.path().join("snapshots"),
        );
        assert!(
            plugin.is_applicable(temp.path()).await,
            "MysqlPlugin must be applicable when mysqldump is on PATH"
        );

        let snapshot_id = plugin
            .snapshot(temp.path(), "DROP TABLE items")
            .await
            .expect("mysql snapshot must succeed against live MySQL 8.0 server");

        conn.run_mysql(
            Some(&conn.database),
            "INSERT INTO items (name) VALUES ('after');",
        );
        assert_eq!(
            conn.run_mysql(
                Some(&conn.database),
                "SELECT GROUP_CONCAT(name ORDER BY id SEPARATOR ',') FROM items;"
            ),
            "before,after",
            "mutation must be visible before rollback"
        );

        plugin
            .rollback(&snapshot_id)
            .await
            .expect("mysql rollback must succeed against live MySQL 8.0 server");

        assert_eq!(
            conn.run_mysql(
                Some(&conn.database),
                "SELECT GROUP_CONCAT(name ORDER BY id SEPARATOR ',') FROM items;"
            ),
            "before",
            "mysql rollback must restore the pre-mutation database contents"
        );

        plugin
            .delete(&snapshot_id)
            .await
            .expect("mysql snapshot delete must succeed");
    }
}

#[tokio::test]
async fn supabase_snapshot_rollback_restores_live_postgres_database() {
    if live_test_enabled(
        "AEGIS_SUPABASE_SNAPSHOT_TESTS",
        "a running PostgreSQL server",
    ) {
        let conn = PgConn::from_env("AEGIS_SUPABASE_TEST_DB", "aegis_supabase_test");
        let temp = TempDir::new().unwrap();
        let config = SupabaseSnapshotConfig {
            project_ref: "aegis-ci-postgres".to_string(),
            db: PostgresSnapshotConfig {
                database: conn.database.clone(),
                host: conn.host.clone(),
                port: conn.port,
                user: conn.user.clone(),
            },
            ..SupabaseSnapshotConfig::default()
        };
        let plugin = SupabasePlugin::new(config, temp.path().join("snapshots"));
        run_pg_rollback_cycle(&conn, &plugin, "supabase").await;
    }
}
