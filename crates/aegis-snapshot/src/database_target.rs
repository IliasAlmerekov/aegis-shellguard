use crate::containment::contain_artifact;
use crate::secure_fs::create_artifact_file;
use crate::{SnapshotError, snapshot_id};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SnapshotError>;

pub(crate) struct DatabaseTarget {
    pub(crate) database: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) user: String,
    pub(crate) snapshots_dir: PathBuf,
    pub(crate) prefix: &'static str,
    pub(crate) extension: &'static str,
    pub(crate) plugin: &'static str,
}

pub(crate) struct RollbackTarget {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) user: String,
    pub(crate) dump_path: PathBuf,
}

impl DatabaseTarget {
    pub(crate) fn dump_path_candidate(&self, timestamp: u64, suffix: Option<usize>) -> PathBuf {
        let base_name = format!(
            "{}-{}-{timestamp}",
            self.prefix,
            self.sanitized_database_label()
        );
        let file_name = match suffix {
            Some(suffix) => format!("{base_name}-{suffix}.{}", self.extension),
            None => format!("{base_name}.{}", self.extension),
        };

        self.snapshots_dir.join(file_name)
    }

    pub(crate) fn reserve_dump_path(&self, timestamp: u64) -> Result<PathBuf> {
        let mut suffix = None;

        loop {
            let dump_path = contain_artifact(
                self.plugin,
                &self.snapshots_dir,
                &self.dump_path_candidate(timestamp, suffix),
            )?;
            match create_artifact_file(self.plugin, &dump_path) {
                Ok(file) => {
                    drop(file);
                    return Ok(dump_path);
                }
                Err(SnapshotError::Io(err)) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    suffix = Some(suffix.map_or(1, |current| current + 1));
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub(crate) fn sanitized_database_label(&self) -> String {
        self.database
            .chars()
            .map(|ch| match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
                _ => '_',
            })
            .collect()
    }

    pub(crate) fn build_snapshot_id(&self, dump_path: &Path) -> String {
        snapshot_id::join_v3(&[
            &snapshot_id::encode_bytes(self.database.as_bytes()),
            &snapshot_id::encode_bytes(self.host.as_bytes()),
            &self.port.to_string(),
            &snapshot_id::encode_bytes(self.user.as_bytes()),
            &snapshot_id::encode_path(dump_path),
        ])
    }

    pub(crate) fn parse_snapshot_id(&self, id: &str) -> Result<Option<RollbackTarget>> {
        let parts = if snapshot_id::is_v3(id) {
            snapshot_id::split_v3(id, 5)?
        } else if snapshot_id::is_v2(id) {
            snapshot_id::split_v2(id, 5)?
        } else {
            return Ok(None);
        };

        let _database = snapshot_id::decode_string(parts[0], "database")?;
        let host = snapshot_id::decode_string(parts[1], "host")?;
        let port =
            parts[2]
                .parse::<u16>()
                .map_err(|_| SnapshotError::InvalidSnapshotIdComponent {
                    component: "port".to_string(),
                    value: parts[2].to_string(),
                })?;
        let user = snapshot_id::decode_string(parts[3], "user")?;
        let dump_path = snapshot_id::decode_path(parts[4], "dump path")?;

        Ok(Some(RollbackTarget {
            host,
            port,
            user,
            dump_path,
        }))
    }
}
