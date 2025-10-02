mod client;
mod entities;
mod gql;
mod models;

use std::collections::HashMap;

use anyhow::Context;

use crate::config::Config;
use client::Client;
pub use client::{
    ZVUK_DEFAULT_COVER_RESIZE_COMMAND, ZVUK_DOWNLOAD_ENDPOINT,
    ZVUK_GRAPHQL_ENDPOINT, ZVUK_HOST, ZVUK_LYRICS_ENDPOINT,
    ZVUK_RELEASES_ENDPOINT, ZVUK_TRACKS_ENDPOINT, ZVUK_USER_AGENT,
};
pub use entities::Quality;

pub fn download(config: &Config) -> anyhow::Result<()> {
    let mut release_ids = Vec::new();
    let mut track_ids = Vec::new();
    let mut book_ids = Vec::new();

    for url in &config.urls {
        if let Some(url) = url.strip_prefix(client::ZVUK_RELEASE_PREFIX) {
            release_ids.push(url.to_owned());
        } else if let Some(url) = url.strip_prefix(client::ZVUK_ABOOK_PREFIX) {
            book_ids.push(url.to_owned());
        } else if let Some(url) = url.strip_prefix(client::ZVUK_TRACKS_PREFIX)
        {
            track_ids.push(url.to_owned());
        } else {
            tracing::warn!(
                "This doesn't look like zvuk.com URL, skipping: {}",
                url
            );
        }
    }

    let client =
        Client::build(config).context("Failed to create zvuk http client")?;

    if !release_ids.is_empty() {
        client.download_albums(&release_ids)?;
    }
    if !track_ids.is_empty() {
        client.download_tracks(&track_ids, &HashMap::new())?;
    }
    if !book_ids.is_empty() {
        client.download_abooks(&book_ids)?;
    }

    Ok(())
}
