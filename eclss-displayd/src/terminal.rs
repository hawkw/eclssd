use super::*;
impl TerminalArgs {
    pub(super) async fn run(self, mut client: Client) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(self.refresh.into());
        loop {
            let fetch = client.fetch().await?;
            println!("{:#?}\n", fetch);
            interval.tick().await;
        }
    }
}
