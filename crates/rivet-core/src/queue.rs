//! Durable queue shared by the GUI, CLI, and browser native host.
use crate::DownloadRequest;
use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct Queue {
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: String,
    pub request: DownloadRequest,
    pub status: String,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub error: Option<String>,
    pub not_before: i64,
    pub created: i64,
    pub last_try: i64,
    pub total_bytes: u64,
    pub category: String,
    pub description: String,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

impl Queue {
    pub fn default_path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("RIVET_DATABASE") {
            let path = PathBuf::from(path);
            ensure!(
                path.is_absolute(),
                "RIVET_DATABASE must be an absolute path"
            );
            return Ok(path);
        }
        Ok(ProjectDirs::from("org", "rivet", "Rivet")
            .context("cannot locate application data directory")?
            .data_local_dir()
            .join("queue.sqlite3"))
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let queue = Self { path };
        // Create the database privately before SQLite creates its WAL/SHM siblings.
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(false).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        drop(options.open(&queue.path)?);
        let mut conn = queue.connect()?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS jobs (
              id TEXT PRIMARY KEY, request TEXT NOT NULL, status TEXT NOT NULL,
              bytes INTEGER NOT NULL DEFAULT 0, sha256 TEXT, error TEXT,
              created INTEGER NOT NULL, not_before INTEGER NOT NULL,
              owner TEXT, lease INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        let transaction =
            conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for (name, declaration) in [
            ("last_try", "INTEGER NOT NULL DEFAULT 0"),
            ("total_bytes", "INTEGER NOT NULL DEFAULT 0"),
            ("category", "TEXT NOT NULL DEFAULT 'auto'"),
            ("description", "TEXT NOT NULL DEFAULT ''"),
        ] {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('jobs') WHERE name=?1)",
                [name],
                |r| r.get(0),
            )?;
            if !exists {
                transaction
                    .execute_batch(&format!("ALTER TABLE jobs ADD COLUMN {name} {declaration}"))?;
            }
        }
        transaction.commit()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&queue.path, std::fs::Permissions::from_mode(0o600))?;
            // SQLite creates sibling WAL/SHM files; keep the containing private data directory private.
            // Caller-selected directories may be shared, so only change the database files here.
            for suffix in ["-wal", "-shm"] {
                let p = PathBuf::from(format!("{}{suffix}", queue.path.display()));
                if p.exists() {
                    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?;
                }
            }
        }
        Ok(queue)
    }
    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }
    pub fn enqueue(&self, request: DownloadRequest, not_before: i64) -> Result<String> {
        request.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        self.connect()?.execute(
            "INSERT INTO jobs(id,request,status,created,not_before) VALUES(?1,?2,'queued',?3,?4)",
            params![id, serde_json::to_string(&request)?, now(), not_before],
        )?;
        Ok(id)
    }
    pub fn list(&self) -> Result<Vec<Job>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id,request,status,bytes,sha256,error,not_before,created,last_try,total_bytes,category,description FROM jobs ORDER BY created,rowid",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
            ))
        })?;
        let mut jobs = Vec::new();
        for row in rows {
            let (
                id,
                request,
                status,
                bytes,
                sha256,
                error,
                not_before,
                created,
                last_try,
                total_bytes,
                category,
                description,
            ) = row?;
            jobs.push(Job {
                id,
                request: serde_json::from_str(&request)?,
                status,
                bytes: bytes.try_into()?,
                sha256,
                error,
                not_before,
                created,
                last_try,
                total_bytes: total_bytes.try_into()?,
                category,
                description,
            });
        }
        Ok(jobs)
    }
    pub fn get(&self, id: &str) -> Result<Job> {
        self.list()?
            .into_iter()
            .find(|j| j.id == id)
            .context("unknown job")
    }
    /// Atomic lease prevents two workers from starting the same job. Expired jobs can be recovered.
    pub fn claim(&self, id: &str, owner: &str) -> Result<bool> {
        Ok(self.connect()?.execute("UPDATE jobs SET status='running',owner=?2,lease=?3,error=NULL,last_try=?4 WHERE id=?1 AND not_before<=?4 AND (status IN ('queued','paused','failed') OR (status='running' AND lease<?4))",
            params![id, owner, now() + 30, now()])? == 1)
    }
    pub fn heartbeat(&self, id: &str, owner: &str, bytes: u64, total_bytes: u64) -> Result<()> {
        let count = self.connect()?.execute(
            "UPDATE jobs SET bytes=?3,lease=?4,total_bytes=?5 WHERE id=?1 AND owner=?2 AND status='running'",
            params![id, owner, i64::try_from(bytes)?, now() + 30, i64::try_from(total_bytes)?],
        )?;
        ensure!(count == 1, "queue lease lost");
        Ok(())
    }
    pub fn finish(
        &self,
        id: &str,
        owner: &str,
        status: &str,
        bytes: u64,
        sha256: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        ensure!(
            matches!(status, "complete" | "paused" | "failed"),
            "invalid terminal status"
        );
        let count = self.connect()?.execute("UPDATE jobs SET status=?3,bytes=?4,sha256=?5,error=?6,owner=NULL,lease=0,total_bytes=CASE WHEN ?3='complete' THEN ?4 ELSE total_bytes END WHERE id=?1 AND owner=?2", params![id, owner, status, i64::try_from(bytes)?, sha256, error])?;
        ensure!(count == 1, "queue lease lost before completion");
        Ok(())
    }
    pub fn retry(&self, id: &str) -> Result<()> {
        let count = self.connect()?.execute("UPDATE jobs SET status='queued',error=NULL,owner=NULL,lease=0 WHERE id=?1 AND (status IN ('paused','failed') OR (status='running' AND lease<?2))", params![id, now()])?;
        ensure!(
            count == 1,
            "job is complete, active, missing, or already queued"
        );
        Ok(())
    }
    pub fn remove(&self, id: &str) -> Result<()> {
        let count = self.connect()?.execute(
            "DELETE FROM jobs WHERE id=?1 AND (status!='running' OR lease<?2)",
            params![id, now()],
        )?;
        ensure!(count == 1, "cannot remove an active or unknown job");
        Ok(())
    }
    pub fn next_queued(&self) -> Result<Option<String>> {
        Ok(self.connect()?.query_row("SELECT id FROM jobs WHERE status='queued' AND not_before<=?1 ORDER BY created,rowid LIMIT 1", [now()], |r| r.get(0)).optional()?)
    }
    pub fn set_metadata(&self, id: &str, category: &str, description: &str) -> Result<()> {
        ensure!(
            category.len() <= 100 && description.len() <= 4096,
            "metadata exceeds limits"
        );
        ensure!(
            self.connect()?.execute(
                "UPDATE jobs SET category=?2,description=?3 WHERE id=?1",
                params![id, category, description]
            )? == 1,
            "unknown job"
        );
        Ok(())
    }
    pub fn schedule(&self, id: &str, not_before: i64) -> Result<()> {
        ensure!(self.connect()?.execute("UPDATE jobs SET not_before=?2,status='queued' WHERE id=?1 AND status IN ('queued','paused','failed')", params![id,not_before])? == 1, "only idle jobs can be scheduled");
        Ok(())
    }
}
