mod entities;
mod models;

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

use crate::config::Config;

pub use entities::Quality;
use entities::{Lyrics, LyricsKind, ReleaseInfo, TrackInfo};

const ZVUK_HOST: &str = "https://zvuk.com";
const ZVUK_RELEASE_PREFIX: &str = "https://zvuk.com/release/";
const ZVUK_TRACKS_PREFIX: &str = "https://zvuk.com/track/";
const ZVUK_RELEASES_URL: &str = "https://zvuk.com/api/tiny/releases";
const ZVUK_TRACKS_URL: &str = "https://zvuk.com/api/tiny/tracks";
const ZVUK_DOWNLOAD_URL: &str = "https://zvuk.com/api/tiny/track/stream";
const ZVUK_LYRICS_URL: &str = "https://zvuk.com/api/tiny/lyrics";

pub const ZVUK_DEFAULT_COVER_RESIZE_COMMAND: &str =
    "magick {source} -define jpeg:extent=1MB {target}";

pub const ZVUK_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

struct Client {
    embed_cover: bool,
    resize_cover: bool,
    resize_cover_limit: u64,
    download_lyrics: bool,
    resize_command: String,
    quality: Quality,
    output_dir: PathBuf,

    pause_between_getting_track_links: Duration,
    default_headers: HeaderMap,
    http: reqwest::blocking::Client,
}

impl Client {
    fn new(config: &Config) -> Self {
        let jar = Jar::default();
        jar.add_cookie_str(
            format!("auth={}", config.token).as_str(),
            &ZVUK_HOST.parse::<Url>().unwrap(),
        );
        let mut default_headers = HeaderMap::new();
        default_headers.append(USER_AGENT, config.user_agent.parse().unwrap());

        Self {
            embed_cover: config.embed_cover,
            resize_cover: config.resize_cover,
            resize_cover_limit: config.resize_cover_limit,
            download_lyrics: config.download_lyrics,
            resize_command: config.resize_command.clone(),
            pause_between_getting_track_links: config
                .pause_between_getting_track_links,
            quality: config.quality,
            output_dir: PathBuf::from(&config.output_dir),

            default_headers,
            http: reqwest::blocking::Client::builder()
                .cookie_provider(jar.into())
                .build()
                .unwrap(),
        }
    }

    fn get_releases_info(
        &self,
        release_ids: &[String],
    ) -> anyhow::Result<HashMap<String, ReleaseInfo>> {
        tracing::info!("Getting releases metadata");
        let response = self
            .http
            .get(ZVUK_RELEASES_URL)
            .query(&[("ids", release_ids.join(","))])
            .headers(self.default_headers.clone())
            .send()
            .context("Failed to download releases metadata")?
            .error_for_status()?;

        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse releses metadata")?;
        tracing::trace!("{ZVUK_RELEASES_URL} response: {body:#?}");

        let result = models::ZvukResponse::deserialize(body)?.result;
        let mut releases = HashMap::with_capacity(result.releases.len());

        for (release_id, release_info) in result.releases {
            releases.insert(release_id.clone(), release_info.try_into()?);
        }

        Ok(releases)
    }

    fn download_albums(&self, release_ids: &[String]) -> anyhow::Result<()> {
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

    fn download_tracks(
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
            .get(ZVUK_TRACKS_URL)
            .query(&[("ids", track_ids.join(","))])
            .headers(self.default_headers.clone())
            .send()
            .context("Failed to donwload tracks metadata")?
            .error_for_status()?;

        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse tracks metadata")?;
        tracing::trace!("{ZVUK_TRACKS_URL} response: {body:#?}");

        let result = models::ZvukResponse::deserialize(body)?.result;
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
            .get(ZVUK_DOWNLOAD_URL)
            .query(&[
                ("quality", effective_quality.to_string().as_str()),
                ("id", track_id),
            ])
            .headers(self.default_headers.clone())
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
            "{ZVUK_DOWNLOAD_URL} response for id={track_id}: {body:#?}"
        );

        let result = models::ZvukDownloadResponse::deserialize(body)?.result;
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
            .get(ZVUK_LYRICS_URL)
            .query(&[("track_id", track_id)])
            .headers(self.default_headers.clone())
            .send()
            .context("Failed to download lyrics")?
            .error_for_status()?;
        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse lyrics")?;
        tracing::trace!("{ZVUK_LYRICS_URL} response: {body:#?}");
        let result = models::ZvukLyricsResponse::deserialize(body)?.result;

        let lyrics_type = if result.type_ == "subtitle" {
            LyricsKind::Subtitle
        } else {
            LyricsKind::Lyrics
        };

        Ok(Lyrics {
            kind: lyrics_type,
            text: result.lyrics,
        })
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
}

pub fn download(config: &Config) -> anyhow::Result<()> {
    let mut release_ids = Vec::new();
    let mut track_ids = Vec::new();

    for url in &config.urls {
        if let Some(url) = url.strip_prefix(ZVUK_RELEASE_PREFIX) {
            release_ids.push(url.to_owned());
        } else if let Some(url) = url.strip_prefix(ZVUK_TRACKS_PREFIX) {
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

#[cfg(target_os = "windows")]
fn sanitize_path(path: &str) -> String {
    path.replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_")
}

#[cfg(not(target_os = "windows"))]
fn sanitize_path(path: &str) -> String {
    path.replace(['/'], "_")
}
