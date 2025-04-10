mod client;
mod entities;
mod models;

use std::collections::HashMap;

use crate::config::Config;
use client::Client;
pub use client::ZVUK_DEFAULT_COVER_RESIZE_COMMAND;
pub use client::ZVUK_USER_AGENT;
pub use entities::Quality;

pub fn download(config: &Config) -> anyhow::Result<()> {
    let mut release_ids = Vec::new();
    let mut track_ids = Vec::new();

    for url in &config.urls {
        if let Some(url) = url.strip_prefix(client::ZVUK_RELEASE_PREFIX) {
            release_ids.push(url.to_owned());
        } else if let Some(url) = url.strip_prefix(client::ZVUK_TRACKS_PREFIX)
        {
            track_ids.push(url.to_owned());
        } else {
            tracing::warn!(
                "This doens't look like zvuk.com URL, skipping: {}",
                url
            );
        }
    }

    let client = Client::new(config);

    if !release_ids.is_empty() {
        client.download_albums(&release_ids)?;
    }
    if !track_ids.is_empty() {
        client.download_tracks(&track_ids, &HashMap::new())?;
    }

    Ok(())
}
