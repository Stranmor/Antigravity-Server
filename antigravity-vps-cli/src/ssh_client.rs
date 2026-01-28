use anyhow::{anyhow, Result};
use async_ssh2_lite::{AsyncSession, SessionConfiguration};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

pub struct SshClientFactory;

impl SshClientFactory {
    pub async fn new() -> Result<Self> {
        Ok(SshClientFactory)
    }

    pub async fn connect(&self, user: &str, host: String) -> Result<SshSession> {
        let tcp = TcpStream::connect(format!("{}:22", host)).await?;
        let config = SessionConfiguration::new();
        let mut session = AsyncSession::new(tcp, config)?;
        session.handshake().await?;
        session.userauth_agent(user).await?;
        Ok(SshSession { session })
    }
}

pub struct SshSession {
    session: AsyncSession<TcpStream>,
}

impl SshSession {
    pub async fn exec_command(&mut self, command: &str) -> Result<String> {
        let mut channel = self.session.channel_session().await?;
        channel.exec(command).await?;
        let mut buf = Vec::new();
        channel.read_to_end(&mut buf).await?;
        String::from_utf8(buf).map_err(|e| anyhow!("Failed to convert output to UTF-8: {}", e))
    }

    pub async fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(None, "Disconnected by client", None)
            .await?;
        Ok(())
    }

    pub async fn upload_file(
        &mut self,
        _remote_host: &str,
        local_path: &PathBuf,
        remote_path: &str,
    ) -> Result<()> {
        let metadata = fs::metadata(local_path).await?;
        let mut channel = self
            .session
            .scp_send(
                Path::new(remote_path),
                0o644, // permissions
                metadata.len(),
                None, // mtime and atime - Option<(u64, u64)>
            )
            .await?;

        let mut local_file = fs::File::open(local_path).await?;
        tokio::io::copy(&mut local_file, &mut channel).await?;

        channel.send_eof().await?;
        channel.wait_eof().await?;
        channel.close().await?;
        channel.wait_close().await?;

        Ok(())
    }

    pub async fn download_file(
        &mut self,
        _remote_host: &str,
        remote_path: &str,
        local_path: &PathBuf,
    ) -> Result<()> {
        let (mut channel, _stat) = self.session.scp_recv(Path::new(remote_path)).await?;

        let mut local_file = fs::File::create(local_path).await?;
        tokio::io::copy(&mut channel, &mut local_file).await?;

        channel.send_eof().await?;
        channel.wait_eof().await?;
        channel.close().await?;
        channel.wait_close().await?;

        Ok(())
    }
}
