use anyhow::Context;
use eclss::storage::Store;
use eclss_api::SensorName;
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};

#[derive(Debug, clap::Parser)]
pub(super) struct StorageArgs {
    /// The path at which to store sensor states.
    #[clap(
        long = "state-dir",
        env = "STATE_DIRECTORY",
        default_value = "/var/lib/eclss"
    )]
    path: PathBuf,
}

impl StorageArgs {
    pub(super) async fn ensure_state_dir(self) -> anyhow::Result<StateDir> {
        tokio::fs::create_dir_all(&self.path)
            .await
            .with_context(|| {
                format!(
                    "failed to ensure state directory {} exists",
                    self.path.display()
                )
            })?;
        Ok(StateDir {
            path: Arc::new(self.path),
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct StateDir {
    path: Arc<PathBuf>,
}

impl StateDir {
    pub(super) fn sensor_state(
        &self,
        sensor: SensorName,
    ) -> impl Future<Output = anyhow::Result<StateFile>> + Send + Sync + 'static {
        let path = self.path.join(format!("{sensor}.toml"));
        async move {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .await
                .with_context(|| format!("failed to open {}", path.display()))?;
            Ok(StateFile { file, path })
        }
    }
}

pub(super) struct StateFile {
    file: File,
    path: PathBuf,
}

impl Store for StateFile {
    type Error = anyhow::Error;
    async fn load<T: DeserializeOwned>(&mut self) -> Result<Option<T>, Self::Error> {
        let mut buf = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut self.file, &mut buf)
            .await
            .with_context(|| format!("failed to read state file {}", self.path.display()))?;

        if buf.is_empty() {
            return Ok(None);
        }

        toml::from_str::<T>(&buf)
            .map(Some)
            .with_context(|| format!("failed to parse state file {}", self.path.display()))
    }

    async fn store<T: Serialize>(&mut self, state: &T) -> Result<(), Self::Error> {
        let buf = toml::to_string_pretty(&state).context("failed to serialize state")?;
        tokio::io::AsyncWriteExt::write_all(&mut self.file, buf.as_bytes())
            .await
            .with_context(|| format!("failed to write to state file {}", self.path.display()))
    }
}
