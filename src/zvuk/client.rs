use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use audiotags::{
    traits::AudioTagWrite, AudioTag, FlacTag, Id3v2Tag, MimeType, Picture,
};
use chrono::{Datelike, NaiveDate};
use id3::{frame, TagLike};
use reqwest::{
    cookie::Jar,
    header::{HeaderMap, USER_AGENT},
    Url,
};
use serde::Deserialize;

use super::entities::{BookChapter, Lyrics, ReleaseInfo, TrackInfo};
use super::gql;
use super::Quality;
use crate::config::Config;

pub const ZVUK_HOST: &str = "https://zvuk.com";
pub const ZVUK_RELEASES_ENDPOINT: &str = "/api/tiny/releases";
pub const ZVUK_TRACKS_ENDPOINT: &str = "/api/tiny/tracks";
pub const ZVUK_DOWNLOAD_ENDPOINT: &str = "/api/tiny/track/stream";
pub const ZVUK_LYRICS_ENDPOINT: &str = "/api/tiny/lyrics";
pub const ZVUK_GRAPHQL_ENDPOINT: &str = "/api/v1/graphql";

pub(super) const ZVUK_RELEASE_PREFIX: &str = "https://zvuk.com/release/";
pub(super) const ZVUK_TRACKS_PREFIX: &str = "https://zvuk.com/track/";
pub(super) const ZVUK_ABOOK_PREFIX: &str = "https://zvuk.com/abook/";

pub const ZVUK_DEFAULT_COVER_RESIZE_COMMAND: &str =
    "magick {source} -define jpeg:extent=1MB {target}";

pub const ZVUK_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub(super) struct Client {
    embed_cover: bool,
    resize_cover: bool,
    resize_cover_limit: u64,
    download_lyrics: bool,
    resize_command: String,
    quality: Quality,
    output_dir: PathBuf,

    pause_between_getting_track_links: Duration,
    zvuk_releases_url: Url,
    zvuk_tracks_url: Url,
    zvuk_download_url: Url,
    zvuk_lyrics_url: Url,
    zvuk_graphql_url: Url,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn build(config: &Config) -> anyhow::Result<Self> {
        fn join(host: &Url, path: &str) -> anyhow::Result<Url> {
            host.join(path)
                .with_context(|| format!("Incorrect endpoint: {path}"))
        }

        let zvuk_host =
            config.zvuk_host.parse::<Url>().with_context(|| {
                format!("Incorrect host: {}", config.zvuk_host)
            })?;
        let zvuk_releases_url =
            join(&zvuk_host, &config.zvuk_releases_endpoint)?;
        let zvuk_tracks_url = join(&zvuk_host, &config.zvuk_tracks_endpoint)?;
        let zvuk_download_url =
            join(&zvuk_host, &config.zvuk_download_endpoint)?;
        let zvuk_lyrics_url = join(&zvuk_host, &config.zvuk_lyrics_endpoint)?;
        let zvuk_graphql_url =
            join(&zvuk_host, &config.zvuk_graphql_endpoint)?;

        let jar = Jar::default();
        jar.add_cookie_str(
            format!("auth={}", config.token).as_str(),
            &zvuk_host,
        );
        let mut default_headers = HeaderMap::new();
        default_headers.append(USER_AGENT, config.user_agent.parse()?);

        Ok(Self {
            embed_cover: config.embed_cover,
            resize_cover: config.resize_cover,
            resize_cover_limit: config.resize_cover_limit,
            download_lyrics: config.download_lyrics,
            resize_command: config.resize_command.clone(),
            pause_between_getting_track_links: config
                .pause_between_getting_track_links,
            quality: config.quality,
            output_dir: PathBuf::from(&config.output_dir),

            zvuk_releases_url,
            zvuk_tracks_url,
            zvuk_download_url,
            zvuk_lyrics_url,
            zvuk_graphql_url,

            http: reqwest::blocking::Client::builder()
                .cookie_provider(jar.into())
                .default_headers(default_headers)
                .timeout(config.request_timeout)
                .build()?,
        })
    }

    fn get_releases_info(
        &self,
        release_ids: &[String],
    ) -> anyhow::Result<HashMap<String, super::entities::ReleaseInfo>> {
        tracing::info!("Getting releases metadata");
        let response = self
            .http
            .get(self.zvuk_releases_url.clone())
            .query(&[("ids", release_ids.join(","))])
            .send()
            .context("Failed to download releases metadata")?
            .error_for_status()?;

        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse releases metadata")?;
        tracing::trace!("{0} response: {body:#?}", self.zvuk_releases_url);

        let result = super::models::ZvukResponse::deserialize(body)?.result;
        let mut releases = HashMap::with_capacity(result.releases.len());

        for (release_id, release_info) in result.releases {
            releases.insert(release_id.clone(), release_info.try_into()?);
        }

        Ok(releases)
    }

    pub fn download_albums(
        &self,
        release_ids: &[String],
    ) -> anyhow::Result<()> {
        let mut track_ids = Vec::new();
        let releases = self
            .get_releases_info(release_ids)
            .context("Failed to get releases metadata")?;

        for release_info in releases.values() {
            track_ids.extend(release_info.track_ids.clone());
        }

        self.download_tracks(&track_ids, &releases)
            .context("Failed to download tracks")?;
        Ok(())
    }

    pub fn download_tracks(
        &self,
        track_ids: &[String],
        releases: &HashMap<String, ReleaseInfo>,
    ) -> anyhow::Result<()> {
        let metadata = self
            .get_tracks_metadata(track_ids)
            .context("Failed to get tracks metadata")?;
        let links = self
            .get_tracks_links(&metadata)
            .context("Failed to get tracks download links")?;

        if metadata.len() != links.len() {
            return Err(anyhow::anyhow!(
                "metadata and links have different length"
            ));
        }
        let releases_ = if releases.is_empty() {
            let mut release_ids = HashSet::new();
            for track_info in metadata.values() {
                release_ids.insert(track_info.release_id.clone());
            }
            let release_ids = release_ids.into_iter().collect::<Vec<String>>();
            &self
                .get_releases_info(&release_ids)
                .context("Failed to get releases metadata")?
        } else {
            releases
        };

        for (track_id, track_info) in metadata {
            let (link, actual_quality) =
                links.get(&track_id).context("no link")?;
            let result = self.get_and_save_track(
                link.as_str(),
                &track_info,
                releases_
                    .get(&track_info.release_id)
                    .context("no release info")?,
                *actual_quality,
            );
            if let Err(e) = result {
                tracing::warn!(
                    "Failed to download and process track id={track_id}: {e:#}"
                );
            }
        }
        Ok(())
    }

    fn get_tracks_metadata(
        &self,
        track_ids: &[String],
    ) -> anyhow::Result<HashMap<String, TrackInfo>> {
        tracing::info!("Getting tracks metadata");
        let response = self
            .http
            .get(self.zvuk_tracks_url.clone())
            .query(&[("ids", track_ids.join(","))])
            .send()
            .context("Failed to download tracks metadata")?
            .error_for_status()?;

        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse tracks metadata")?;
        tracing::trace!("{0} response: {body:#?}", self.zvuk_tracks_url);

        let result = super::models::ZvukResponse::deserialize(body)?.result;
        let mut tracks = HashMap::with_capacity(result.tracks.len());

        for (track_id, track_info) in result.tracks {
            tracks.insert(track_id.clone(), track_info.try_into()?);
        }

        Ok(tracks)
    }

    const fn determine_effective_quality(
        &self,
        track_info: &TrackInfo,
    ) -> Quality {
        match self.quality {
            Quality::Flac if track_info.has_flac => Quality::Flac,
            Quality::Flac | Quality::MP3High => Quality::MP3High, // Fallback from FLAC or if MP3High requested
            Quality::MP3Mid => Quality::MP3Mid, // Must be MP3Mid requested
        }
    }

    fn log_quality_selection(
        &self,
        track_id: &str,
        effective_quality: Quality,
        has_flac: bool,
    ) {
        if effective_quality == self.quality {
            tracing::debug!(
                "Track id {track_id}: Using requested {} quality (FLAC available: {})",
                effective_quality,
                has_flac
            );
        } else {
            tracing::info!(
                "Track id {track_id}: Falling back to {} quality (FLAC available: {})",
                effective_quality,
                has_flac
            );
        }
    }

    fn fetch_track_link(
        &self,
        track_id: &str,
        effective_quality: Quality,
    ) -> anyhow::Result<String> {
        let response = self
            .http
            .get(self.zvuk_download_url.clone())
            .query(&[
                ("quality", effective_quality.to_string().as_str()),
                ("id", track_id),
            ])
            .send()
            .with_context(|| {
                format!("Failed to download track link for id={track_id}")
            })?
            .error_for_status()?;

        let body =
            response.json::<serde_json::Value>().with_context(|| {
                format!("Failed to parse track link for id={track_id}")
            })?;
        tracing::trace!(
            "{0} response for id={track_id}: {body:#?}",
            self.zvuk_download_url
        );

        let result =
            super::models::ZvukDownloadResponse::deserialize(body)?.result;
        Ok(result.stream)
    }

    fn get_tracks_links(
        &self,
        metadata: &HashMap<String, TrackInfo>,
    ) -> anyhow::Result<HashMap<String, (String, Quality)>> {
        tracing::info!(
            "Getting download urls (requested: {} quality)",
            self.quality
        );
        let mut urls = HashMap::new();

        for (track_id, track_info) in metadata {
            let effective_quality =
                self.determine_effective_quality(track_info);
            self.log_quality_selection(
                track_id,
                effective_quality,
                track_info.has_flac,
            );

            let link = self.fetch_track_link(track_id, effective_quality)?;

            urls.insert(track_id.clone(), (link, effective_quality));

            std::thread::sleep(self.pause_between_getting_track_links);
        }
        Ok(urls)
    }

    fn get_lyrics(
        &self,
        track_id: &str,
        path: &Path,
    ) -> anyhow::Result<Lyrics> {
        tracing::info!("Getting lyrics for {}", path.display());
        let response = self
            .http
            .get(self.zvuk_lyrics_url.clone())
            .query(&[("track_id", track_id)])
            .send()
            .context("Failed to download lyrics")?
            .error_for_status()?;
        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse lyrics")?;
        tracing::trace!("{0} response: {body:#?}", self.zvuk_lyrics_url);
        let result =
            super::models::ZvukLyricsResponse::deserialize(body)?.result;
        result.try_into()
    }

    fn download_cover(&self, url: &str, path: &Path) -> anyhow::Result<()> {
        if !path.try_exists()? {
            tracing::info!("Downloading cover {}", path.display());
            let response = self
                .http
                .get(url)
                .send()
                .context("Failed to download cover")?
                .error_for_status()?;
            std::fs::write(path, response.bytes()?)?;
        }

        if self.resize_cover
            && std::fs::metadata(path)?.len() > self.resize_cover_limit
        {
            tracing::debug!("Resizing cover {}", path.display());

            let path_str =
                path.to_str().context("Failed to convert path to str")?;
            let command_str = self
                .resize_command
                .split_whitespace()
                .map(|x| {
                    x.replace("{source}", path_str)
                        .replace("{target}", path_str)
                })
                .collect::<Vec<String>>();
            let (command, args) = command_str
                .split_first()
                .context("Failed to parse resize command")?;

            let status = std::process::Command::new(command)
                .args(args)
                .status()
                .context("Failed to run resize command")?;
            if !status.success() {
                return Err(anyhow::anyhow!("Failed to resize cover"));
            }
        }
        Ok(())
    }

    fn get_and_save_track(
        &self,
        url: &str,
        track_info: &TrackInfo,
        release_info: &ReleaseInfo,
        actual_quality: Quality,
    ) -> anyhow::Result<()> {
        let directory_name = sanitize_path(&format!(
            "{} - {} ({})",
            release_info.author,
            release_info.album,
            release_info.date.chars().take(4).collect::<String>()
        ));
        let directory_path = self.output_dir.join(directory_name);

        std::fs::create_dir_all(&directory_path).with_context(|| {
            format!("Failed to create directory {}", directory_path.display())
        })?;

        let cover_path = directory_path.join("cover.jpg");
        self.download_cover(&track_info.image, &cover_path)
            .context("Failed to download and process album cover")?;

        let filename = sanitize_path(&format!(
            "{:02} - {}.{}",
            track_info.number,
            track_info.name,
            actual_quality.extension()
        ));
        let filename = PathBuf::from(filename);
        let filepath = directory_path.join(filename);

        if filepath.exists() {
            tracing::info!(
                "File already exists, skipping: {}",
                filepath.display()
            );
            return Ok(());
        }

        tracing::info!("Downloading {}", filepath.display());

        let response = self
            .http
            .get(url)
            .send()
            .context("Failed to download track")?
            .error_for_status()?;
        std::fs::write(
            &filepath,
            response.bytes().context("Failed to read track data")?,
        )
        .context("Failed to save track on disk")?;

        self.write_tags(
            &filepath,
            &cover_path,
            track_info,
            release_info,
            actual_quality,
        )?;

        Ok(())
    }

    fn write_tags(
        &self,
        filepath: &Path,
        cover_path: &PathBuf,
        track_info: &TrackInfo,
        release_info: &ReleaseInfo,
        actual_quality: Quality,
    ) -> anyhow::Result<()> {
        let mut tags: Box<dyn AudioTag + Send + Sync> = match actual_quality {
            Quality::Flac => FlacTag::read_from_path(filepath).map_or_else(
                |_| {
                    tracing::trace!("Failed to read FLAC tag from file");
                    Box::new(FlacTag::new())
                },
                Box::new,
            ),
            Quality::MP3High | Quality::MP3Mid => {
                Id3v2Tag::read_from_path(filepath).map_or_else(
                    |_| {
                        tracing::trace!("Failed to read ID3v2 tag from file");
                        Box::new(Id3v2Tag::new())
                    },
                    Box::new,
                )
            },
        };

        tags.set_artist(&track_info.author);
        tags.set_title(&track_info.name);
        tags.set_album_title(&release_info.album);
        tags.set_track_number(track_info.number.try_into()?);
        tags.set_total_tracks(release_info.track_count.try_into()?);
        tags.set_genre(&track_info.genre);

        if let Ok(date) =
            NaiveDate::parse_from_str(&release_info.date, "%Y%m%d")
        {
            tags.set_date(id3::Timestamp {
                year: date.year(),
                month: u8::try_from(date.month()).ok(),
                day: u8::try_from(date.day()).ok(),
                hour: None,
                minute: None,
                second: None,
            });
            tags.set_year(date.year());
        }

        if self.embed_cover {
            let cover = Picture {
                mime_type: MimeType::Jpeg,
                data: &std::fs::read(cover_path)
                    .context("Failed to read cover file for embedding")?,
            };
            tags.set_album_cover(cover);
        }

        let lyrics = if self.download_lyrics && track_info.lyrics {
            let lyrics = self
                .get_lyrics(&track_info.track_id, filepath)
                .context("Failed to get lyrics")?;
            if lyrics.text.is_empty() {
                tracing::warn!("No lyrics for {}", filepath.display());
            }
            Some(lyrics)
        } else {
            None
        };

        match actual_quality {
            Quality::Flac => {
                Self::write_extra_tags_flac(
                    filepath,
                    track_info,
                    release_info,
                    tags,
                    lyrics.as_ref(),
                )?;
            },
            Quality::MP3High | Quality::MP3Mid => {
                Self::write_extra_tags_mp3(
                    filepath,
                    track_info,
                    release_info,
                    tags,
                    lyrics.as_ref(),
                )?;
            },
        }

        Ok(())
    }

    fn write_extra_tags_flac(
        filepath: &Path,
        track_info: &TrackInfo,
        release_info: &ReleaseInfo,
        tags: Box<dyn AudioTag + Send + Sync>,
        lyrics: Option<&Lyrics>,
    ) -> anyhow::Result<()> {
        let mut flactag: metaflac::Tag = tags.into();
        let vorbis_tags = flactag.vorbis_comments_mut();

        vorbis_tags.set("COPYRIGHT", vec![&release_info.label]);
        vorbis_tags.set("RELEASE_ID", vec![&track_info.release_id]);
        vorbis_tags.set("TRACK_ID", vec![&track_info.track_id]);

        if let Some(lyrics) = lyrics {
            if !lyrics.text.is_empty() {
                vorbis_tags.set_lyrics(vec![&lyrics.text]);
            }
        }

        let mut tags: FlacTag = flactag.into();
        tags.write_to_path(
            filepath.to_str().context("filepath is not valid string")?,
        )
        .context("Failed to write tags to file")?;
        Ok(())
    }

    fn write_extra_tags_mp3(
        filepath: &Path,
        _track_info: &TrackInfo,
        release_info: &ReleaseInfo,
        tags: Box<dyn AudioTag + Send + Sync>,
        lyrics: Option<&Lyrics>,
    ) -> anyhow::Result<()> {
        let mut mp3tags: id3::Tag = tags.into();

        mp3tags.set_text("TCOP", &release_info.label);

        if let Some(lyrics) = lyrics {
            if !lyrics.text.is_empty() {
                mp3tags.add_frame(frame::Lyrics {
                    lang: String::new(),
                    description: String::new(),
                    text: lyrics.text.clone(),
                });
            }
        }

        let mut tags: Id3v2Tag = mp3tags.into();
        tags.write_to_path(
            filepath.to_str().context("filepath is not valid string")?,
        )
        .context("Failed to write tags to file")?;
        Ok(())
    }

    pub fn download_abooks(&self, book_ids: &[String]) -> anyhow::Result<()> {
        let metadata = self
            .get_books_metadata(book_ids)
            .context("Failed to get books metadata")?;

        let links = self
            .get_chapter_links(&metadata)
            .context("Failed to get audiobooks download links")?;

        if metadata.len() != links.len() {
            return Err(anyhow::anyhow!(
                "metadata and links have different length"
            ));
        }

        for ((chapter_id, chapter_info), chapter_link) in
            metadata.into_iter().zip(links)
        {
            let result = self
                .get_and_save_chapter(chapter_link.as_str(), &chapter_info);
            if let Err(e) = result {
                tracing::warn!("Failed to download and process chapter id={chapter_id}: {e:#}");
            }
        }

        Ok(())
    }

    fn get_and_save_chapter(
        &self,
        url: &str,
        chapter_info: &BookChapter,
    ) -> anyhow::Result<()> {
        let directory_name = sanitize_path(&format!(
            "{} - {}",
            chapter_info.author, chapter_info.book_title,
        ));
        let directory_path = self.output_dir.join(directory_name);

        std::fs::create_dir_all(&directory_path).with_context(|| {
            format!("Failed to create directory {}", directory_path.display())
        })?;

        let cover_path = directory_path.join("cover.jpg");
        self.download_cover(&chapter_info.image, &cover_path)
            .context("Failed to download and process album cover")?;

        let filename = sanitize_path(&format!(
            "{:02} - {}.{}",
            chapter_info.number,
            chapter_info.title,
            Quality::MP3Mid.extension(),
        ));
        let filename = PathBuf::from(filename);
        let filepath = directory_path.join(filename);

        if filepath.exists() {
            tracing::info!(
                "File already exists, skipping: {}",
                filepath.display()
            );
            return Ok(());
        }

        tracing::info!("Downloading {}", filepath.display());

        let response = self
            .http
            .get(url)
            .send()
            .context("Failed to download track")?
            .error_for_status()?;
        std::fs::write(
            &filepath,
            response.bytes().context("Failed to read track data")?,
        )
        .context("Failed to save track on disk")?;

        let mut tags: Box<dyn AudioTag> = Id3v2Tag::read_from_path(&filepath)
            .map_or_else(
                |_| {
                    tracing::trace!("Failed to read ID3v2 tag from file");
                    Box::new(Id3v2Tag::new())
                },
                Box::new,
            );

        tags.set_artist(&chapter_info.author);
        tags.set_title(&chapter_info.title);
        tags.set_album_title(&chapter_info.book_title);
        tags.set_track_number(chapter_info.number.try_into()?);

        if self.embed_cover {
            let cover = Picture {
                mime_type: MimeType::Jpeg,
                data: &std::fs::read(cover_path)
                    .context("Failed to read cover file for embedding")?,
            };
            tags.set_album_cover(cover);
        }

        tags.write_to_path(
            filepath.to_str().context("filepath is not valid string")?,
        )
        .context("Failed to write tags to file")?;

        Ok(())
    }

    fn get_books_metadata(
        &self,
        book_ids: &[String],
    ) -> anyhow::Result<HashMap<String, BookChapter>> {
        tracing::info!("Getting books metadata");
        let request = serde_json::json!({
            "query": gql::ZVUK_GQL_GET_BOOK_CHAPTERS_QUERY,
            "variables": {
                "ids": book_ids
            },
            "operationName": "getBookChapters"
        });
        let response = self
            .http
            .post(self.zvuk_graphql_url.clone())
            .json(&request)
            .send()
            .context("Failed to get books metadata")?
            .error_for_status()?;
        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse books metadata")?;
        tracing::trace!("{0} response: {body:#?}", self.zvuk_graphql_url);

        let result = super::models::ZvukGQLResponse::deserialize(body)?.data;
        let Some(result) = result.get_books else {
            return Err(anyhow::anyhow!("No book info in response"));
        };
        let mut chapters = HashMap::with_capacity(result.len());

        for book in result {
            for chapter in book.chapters {
                chapters.insert(chapter.id.clone(), chapter.try_into()?);
            }
        }

        Ok(chapters)
    }

    fn get_chapter_links(
        &self,
        metadata: &HashMap<String, BookChapter>,
    ) -> anyhow::Result<Vec<String>> {
        tracing::info!("Getting download urls");
        let mut links = Vec::with_capacity(metadata.len());

        let chapter_ids: Vec<_> = metadata.keys().collect();
        let request = serde_json::json!({
            "query": gql::ZVUK_GQL_GET_STREAM,
            "variables": {
                "includeFlacDrm": false,
                "ids": chapter_ids
            },
            "operationName": "getStream"
        });
        let response = self
            .http
            .post(self.zvuk_graphql_url.clone())
            .json(&request)
            .send()
            .context("Failed to get audiobook urls")?
            .error_for_status()?;
        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse urls")?;
        tracing::trace!("{0} response: {body:#?}", self.zvuk_graphql_url);

        let result = super::models::ZvukGQLResponse::deserialize(body)?.data;
        let Some(result) = result.media_contents else {
            return Err(anyhow::anyhow!("No media contents in response"));
        };

        for content in result {
            links.push(content.stream.mid);
        }

        Ok(links)
    }
}

#[cfg(target_os = "windows")]
fn sanitize_path(path: &str) -> String {
    path.replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_")
}

#[cfg(not(target_os = "windows"))]
fn sanitize_path(path: &str) -> String {
    path.replace(['/'], "_")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use clap::Parser;
    use httpmock::prelude::*;
    use serde_json::json;

    use super::*;

    const MOCK_TRACK_ID: &str = "1";
    const MOCK_RELEASE_ID: &str = "99";
    const MOCK_BOOK_ID: &str = "00";
    const MOCK_CHAPTER_ID: &str = "88";
    const MOCK_LYRICS: &str = "mocked lyrics";
    const MOCK_AUDIO_URL: &str = "/file.flac";
    const MOCK_COVER_URL: &str = "/file.jpg";

    fn lyricks_mock<'s>(
        server: &'s MockServer,
        path: &str,
    ) -> httpmock::Mock<'s> {
        server.mock(|when, then| {
            when.method(GET)
                .path(path)
                .query_param("track_id", MOCK_TRACK_ID);
            then.status(200).json_body(json!({
                "result": {
                    "lyrics": MOCK_LYRICS,
                    "type": "lyrics"
                }
            }));
        })
    }

    fn tracks_mock<'s>(
        server: &'s MockServer,
        path: &str,
    ) -> httpmock::Mock<'s> {
        server.mock(|when, then| {
            when.method(GET)
                .path(path)
                .query_param("ids", MOCK_TRACK_ID);
            then.status(200).json_body(json!({
                "result": {
                    "releases": {},
                    "tracks": {
                        MOCK_TRACK_ID: {
                            "artist_ids": [],
                            "artist_names": [],
                            "availability": 1,
                            "condition": "",
                            "credits": "Some artist",
                            "duration": 30,
                            "explicit": false,
                            "genres": [],
                            "has_flac": true,
                            "highest_quality": "flac",
                            "id": MOCK_TRACK_ID.parse::<i64>().unwrap(),
                            "image": {
                                "palette": "",
                                "palette_bottom": "",
                                "src": server.url(MOCK_COVER_URL),
                            },
                            "lyrics": true,
                            "position": 1,
                            "price": 1,
                            "release_id": MOCK_RELEASE_ID.parse::<i64>().unwrap(),
                            "release_title": "Some release title",
                            "search_credits": "",
                            "search_title": "",
                            "template": "",
                            "title": "Some track title"
                        }
                    }
                }
            }));
        })
    }

    fn release_mock<'s>(
        server: &'s MockServer,
        path: &str,
    ) -> httpmock::Mock<'s> {
        server.mock(|when, then| {
            when.method(GET)
                .path(path)
                .query_param("ids", MOCK_RELEASE_ID);
            then.status(200).json_body(json!({
                "result": {
                    "tracks": {},
                    "releases": {
                        MOCK_RELEASE_ID: {
                            "artist_ids": [],
                            "artist_names": [],
                            "availability": 1,
                            "credits": "Some artist",
                            "date": 1,
                            "explicit": false,
                            "genre_ids": [],
                            "has_image": true,
                            "id": MOCK_RELEASE_ID.parse::<i64>().unwrap(),
                            "image": {
                                "palette": "",
                                "palette_bottom": "",
                                "src": server.url(MOCK_COVER_URL),
                            },
                            "label_id": 1,
                            "search_credits": "search credits",
                            "search_title": "search title",
                            "template": "",
                            "title": "Some release title",
                            "track_ids": [MOCK_TRACK_ID.parse::<i64>().unwrap()],
                            "type": "",
                        }
                    }
                }
            }));
        })
    }

    fn download_link_mock<'s>(
        server: &'s MockServer,
        path: &str,
    ) -> httpmock::Mock<'s> {
        server.mock(|when, then| {
            when.method(GET)
                .path(path)
                .query_param("quality", "flac")
                .query_param("id", MOCK_TRACK_ID);
            then.status(200).json_body(json!({
                "result": {
                    "expire": 0,
                    "expire_delta": 0,
                    "stream": server.url(MOCK_AUDIO_URL)
                }
            }));
        })
    }

    fn books_mock<'s>(
        server: &'s MockServer,
        path: &str,
    ) -> httpmock::Mock<'s> {
        server.mock(|when, then| {
            when.method(POST).path(path).json_body_includes(r#"
                {
                    "operationName": "getBookChapters"
                }
            "#);
            then.status(200).json_body(json!({"data": {
                "getBooks": [
                    {
                        "title": "Some book title",
                        "explicit": false,
                        "chapters": [
                            {
                                "id": MOCK_CHAPTER_ID,
                                "title": "Some chapter title",
                                "availability": 1,
                                "duration": 30,
                                "image": {"src": server.url(MOCK_COVER_URL)},
                                "book": {
                                    "id": MOCK_BOOK_ID,
                                    "title": "Some book title",
                                    "explicit": false
                                },
                                "bookAuthors": [{
                                    "id": "77",
                                    "rname": "Rname",
                                    "image": {"src": server.url(MOCK_COVER_URL)}
                                }],
                                "position": 1,
                                "__typename": "",
                            }
                        ]
                    }
                ]
            }}));
        })
    }

    fn chapter_link_mock<'s>(
        server: &'s MockServer,
        path: &str,
    ) -> httpmock::Mock<'s> {
        server.mock(|when, then| {
            when.method(POST).path(path).json_body_includes(
                r#"
                {
                    "operationName": "getStream"
                }
            "#,
            );
            then.status(200).json_body(json!({"data": {
                "media_contents": [
                    {
                        "__typename": "",
                        "stream": {
                            "expire": "0",
                            "mid": server.url(MOCK_AUDIO_URL),
                            "type": "flac",
                        },
                    }
                ]
            }}));
        })
    }

    fn audio_mock(server: &MockServer) -> httpmock::Mock<'_> {
        server.mock(|when, then| {
            when.method(GET).path(MOCK_AUDIO_URL);
            then.status(200).body("ohi");
        })
    }

    fn cover_mock(server: &MockServer) -> httpmock::Mock<'_> {
        server.mock(|when, then| {
            when.method(GET).path(MOCK_COVER_URL);
            then.status(200).body("ohi");
        })
    }

    #[test]
    fn get_lyrics() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        let lyrics_mocked =
            lyricks_mock(&server, &config.zvuk_lyrics_endpoint);
        config.zvuk_host = server.base_url();
        let c = Client::build(&config)?;

        // execute
        let result = c.get_lyrics(MOCK_TRACK_ID, Path::new("/tmp/1"))?;

        // assert
        lyrics_mocked.assert();
        assert_eq!(result.text, MOCK_LYRICS);

        Ok(())
    }

    #[test]
    fn get_release_info() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        let release_mocked =
            release_mock(&server, &config.zvuk_releases_endpoint);
        config.zvuk_host = server.base_url();
        let c = Client::build(&config)?;

        // execute
        let result = c.get_releases_info(&[MOCK_RELEASE_ID.to_owned()])?;

        // assert
        release_mocked.assert();
        assert_eq!(result[MOCK_RELEASE_ID].author, "Some artist");
        assert_eq!(result[MOCK_RELEASE_ID].track_ids, &[MOCK_TRACK_ID]);

        Ok(())
    }

    #[test]
    fn get_track_metadata() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        let track_mocked = tracks_mock(&server, &config.zvuk_tracks_endpoint);
        config.zvuk_host = server.base_url();
        let c = Client::build(&config)?;

        // execute
        let result = c.get_tracks_metadata(&[MOCK_TRACK_ID.to_owned()])?;

        // assert
        track_mocked.assert();
        assert_eq!(result[MOCK_TRACK_ID].author, "Some artist");
        assert_eq!(result[MOCK_TRACK_ID].track_id, MOCK_TRACK_ID);

        Ok(())
    }

    #[test]
    fn get_download_link() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        let download_mocked =
            download_link_mock(&server, &config.zvuk_download_endpoint);
        config.zvuk_host = server.base_url();
        config.pause_between_getting_track_links = Duration::from_secs(0);
        let c = Client::build(&config)?;

        let metadata = HashMap::from_iter(vec![(
            MOCK_TRACK_ID.to_string(),
            TrackInfo {
                author: "Some artist".to_string(),
                name: "Some title".to_string(),
                genre: "Some genre".to_string(),
                number: 1,
                release_id: "1".to_string(),
                track_id: "1".to_string(),
                album: "Some album".to_string(),
                image: String::new(),
                lyrics: false,
                has_flac: true,
            },
        )]);

        // execute
        let result = c.get_tracks_links(&metadata)?;

        // assert
        download_mocked.assert();
        assert_eq!(result[MOCK_TRACK_ID].1, Quality::Flac);

        Ok(())
    }

    #[test]
    fn get_books_metadata() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        let books_mocked = books_mock(&server, &config.zvuk_graphql_endpoint);
        config.zvuk_host = server.base_url();
        let c = Client::build(&config)?;

        // execute
        let result = c.get_books_metadata(&[MOCK_BOOK_ID.to_owned()])?;

        // assert
        books_mocked.assert();
        assert_eq!(result[MOCK_CHAPTER_ID].title, "Some chapter title");

        Ok(())
    }

    #[test]
    fn get_chapter_links() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        let chapter_link_mocked =
            chapter_link_mock(&server, &config.zvuk_graphql_endpoint);
        config.zvuk_host = server.base_url();
        let c = Client::build(&config)?;
        let metadata = HashMap::from_iter(vec![(
            MOCK_CHAPTER_ID.to_string(),
            BookChapter {
                author: "Some book author".to_string(),
                book_title: "Some book title".to_string(),
                title: "Some chapter title".to_string(),
                image: String::new(),
                number: 1,
            },
        )]);

        // execute
        let result = c.get_chapter_links(&metadata)?;

        // assert
        chapter_link_mocked.assert();
        assert_eq!(result[0], server.url(MOCK_AUDIO_URL));

        Ok(())
    }

    #[test]
    fn download_albums() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let tmp_dir = tempfile::tempdir()?;
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        release_mock(&server, &config.zvuk_releases_endpoint);
        tracks_mock(&server, &config.zvuk_tracks_endpoint);
        lyricks_mock(&server, &config.zvuk_lyrics_endpoint);
        download_link_mock(&server, &config.zvuk_download_endpoint);
        audio_mock(&server);
        cover_mock(&server);
        config.zvuk_host = server.base_url();
        config.pause_between_getting_track_links = Duration::from_secs(0);
        config.output_dir = tmp_dir
            .path()
            .to_str()
            .context("filepath is not valid string")?
            .to_string();

        // execute
        let c = Client::build(&config)?;
        c.download_albums(&[MOCK_RELEASE_ID.to_string()])?;

        Ok(())
    }

    #[test]
    fn download_abook() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let tmp_dir = tempfile::tempdir()?;
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            "https://zvuk.com/track/1",
        ])?;
        let server = MockServer::start();
        books_mock(&server, &config.zvuk_graphql_endpoint);
        chapter_link_mock(&server, &config.zvuk_graphql_endpoint);
        audio_mock(&server);
        cover_mock(&server);
        config.zvuk_host = server.base_url();
        config.output_dir = tmp_dir
            .path()
            .to_str()
            .context("filepath is not valid string")?
            .to_string();

        // execute
        let c = Client::build(&config)?;
        c.download_abooks(&[MOCK_BOOK_ID.to_string()])?;

        Ok(())
    }

    #[test]
    fn download() -> Result<(), Box<dyn std::error::Error>> {
        // setup
        let tmp_dir = tempfile::tempdir()?;
        let mut config = Config::try_parse_from(vec![
            "zvul-dl",
            "--token=1",
            &format!("https://zvuk.com/track/{MOCK_TRACK_ID}"),
            &format!("https://zvuk.com/release/{MOCK_RELEASE_ID}"),
            &format!("https://zvuk.com/abook/{MOCK_BOOK_ID}"),
        ])?;
        let server = MockServer::start();
        release_mock(&server, &config.zvuk_releases_endpoint);
        tracks_mock(&server, &config.zvuk_tracks_endpoint);
        lyricks_mock(&server, &config.zvuk_lyrics_endpoint);
        download_link_mock(&server, &config.zvuk_download_endpoint);
        books_mock(&server, &config.zvuk_graphql_endpoint);
        chapter_link_mock(&server, &config.zvuk_graphql_endpoint);
        audio_mock(&server);
        cover_mock(&server);
        config.zvuk_host = server.base_url();
        config.pause_between_getting_track_links = Duration::from_secs(0);
        config.output_dir = tmp_dir
            .path()
            .to_str()
            .context("filepath is not valid string")?
            .to_string();

        // execute
        crate::zvuk::download(&config)?;

        Ok(())
    }
}
