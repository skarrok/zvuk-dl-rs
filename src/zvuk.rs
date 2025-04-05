use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use audiotags::{
    traits::AudioTagWrite, AudioTag, FlacTag, Id3v2Tag, MimeType, Picture,
};
use chrono::{Datelike, NaiveDate};
use clap::ValueEnum;
use id3::{frame, TagLike};
use reqwest::{
    cookie::Jar,
    header::{HeaderMap, USER_AGENT},
    Url,
};
use serde::Serialize;

use crate::config::Config;

const ZVUK_HOST: &str = "https://zvuk.com";
const ZVUK_RELEASE_PREFIX: &str = "https://zvuk.com/release/";
const ZVUK_TRACKS_PREFIX: &str = "https://zvuk.com/track/";
const ZVUK_RELEASES_URL: &str = "https://zvuk.com/api/tiny/releases";
const ZVUK_LABELS_URL: &str = "https://zvuk.com/api/tiny/labels";
const ZVUK_TRACKS_URL: &str = "https://zvuk.com/api/tiny/tracks";
const ZVUK_DOWNLOAD_URL: &str = "https://zvuk.com/api/tiny/track/stream";
const ZVUK_LYRICS_URL: &str = "https://zvuk.com/api/tiny/lyrics";

pub const ZVUK_DEFAULT_COVER_RESIZE_COMMAND: &str =
    "magick {source} -define jpeg:extent=1MB {target}";

pub const ZVUK_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug)]
struct ReleaseInfo {
    track_ids: Vec<String>,
    track_count: u32,
    label: String,
    date: String,
    album: String,
    author: String,
}

#[expect(unused)]
#[derive(Debug)]
struct TrackInfo {
    author: String,
    name: String,
    album: String,
    release_id: String,
    track_id: String,
    genre: String,
    number: u32,
    image: String,
    lyrics: bool,
    has_flac: bool,
}

#[derive(ValueEnum, Debug, Clone, Serialize, PartialEq)]
pub enum Quality {
    Flac,
    // 320 kbps
    MP3High,
    // 128 kbps
    MP3Mid,
}

impl Quality {
    fn extension(&self) -> String {
        let string = match self {
            Self::Flac => "flac",
            Self::MP3High | Self::MP3Mid => "mp3",
        };
        String::from(string)
    }
}

impl Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Flac => write!(f, "flac"),
            Self::MP3High => write!(f, "high"),
            Self::MP3Mid => write!(f, "mid"),
        }
    }
}

enum LyricsKind {
    Subtitle,
    Lyrics,
}

#[expect(unused)]
struct Lyrics {
    kind: LyricsKind,
    text: String,
}

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
            quality: config.quality.clone(),
            output_dir: PathBuf::from(&config.output_dir),

            default_headers,
            http: reqwest::blocking::Client::builder()
                .cookie_provider(jar.into())
                .build()
                .unwrap(),
        }
    }

    fn get_labels_info(
        &self,
        label_ids: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        tracing::info!("Getting labels metadata");
        let response = self
            .http
            .get(ZVUK_LABELS_URL)
            .query(&[("ids", label_ids.join(","))])
            .headers(self.default_headers.clone())
            .send()
            .context("Failed to download labels metadata")?;
        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse labels metadata")?;

        tracing::trace!("{ZVUK_LABELS_URL} response: {body:#?}");

        let mut labels = HashMap::new();

        for (label_id, label_info) in body
            .get("result")
            .and_then(|x| x.get("labels"))
            .and_then(|x| x.as_object())
            .context("No labels in labels metadata")?
        {
            labels.insert(
                label_id.clone(),
                label_info
                    .get("title")
                    .and_then(|x| x.as_str())
                    .context("Label title is not a string")?
                    .to_string(),
            );
        }

        Ok(labels)
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
            .context("Failed to download releases metadata")?;

        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse releses metadata")?;

        tracing::trace!("{ZVUK_RELEASES_URL} response: {body:#?}");

        let mut label_ids = Vec::new();
        for (_release_id, release_info) in body
            .get("result")
            .and_then(|x| x.get("releases"))
            .and_then(|x| x.as_object())
            .context("No releases in releases metadata")?
        {
            label_ids.push(
                release_info
                    .get("label_id")
                    .and_then(|x| x.as_number())
                    .context("Label id is not a number")?
                    .to_string(),
            );
        }

        let labels = self.get_labels_info(&label_ids)?;

        let mut releases = HashMap::new();

        for (release_id, release_info) in body
            .get("result")
            .and_then(|x| x.get("releases"))
            .and_then(|x| x.as_object())
            .context("No releases in releases metadata")?
        {
            let track_ids: Vec<_> = release_info
                .get("track_ids")
                .and_then(|x| x.as_array())
                .context("track_ids is not an array")?
                .iter()
                .filter_map(|x| Some(x.as_number()?.to_string()))
                .collect();
            let track_count: u32 = track_ids.len().try_into()?;

            releases.insert(
                release_id.clone(),
                ReleaseInfo {
                    track_ids,
                    track_count,
                    label: labels
                        .get(
                            &release_info
                                .get("label_id")
                                .and_then(|x| x.as_number())
                                .context("label_id is not a number")?
                                .to_string(),
                        )
                        .context("no label info")?
                        .as_str()
                        .to_string(),
                    date: release_info
                        .get("date")
                        .context("no date")?
                        .to_string(),
                    album: release_info
                        .get("title")
                        .and_then(|x| x.as_str())
                        .context("no title")?
                        .to_string(),
                    author: release_info
                        .get("credits")
                        .and_then(|x| x.as_str())
                        .context("credits is not a string")?
                        .to_string(),
                },
            );
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
                actual_quality.clone(),
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
            .context("Failed to donwload tracks metadata")?;

        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse tracks metadata")?;
        tracing::trace!("{ZVUK_TRACKS_URL} response: {body:#?}");

        let mut tracks = HashMap::new();

        for (track_id, track_info) in body
            .get("result")
            .and_then(|x| x.get("tracks"))
            .and_then(|x| x.as_object())
            .context("tracks is not an object")?
        {
            let has_flac = track_info
                .get("has_flac")
                .and_then(serde_json::Value::as_bool)
                .context("has_flac is not bool")?;

            tracks.insert(
                track_id.clone(),
                TrackInfo {
                    author: track_info
                        .get("credits")
                        .and_then(|x| x.as_str())
                        .context("credits is not a string")?
                        .to_string(),
                    name: track_info
                        .get("title")
                        .and_then(|x| x.as_str())
                        .context("title is not a string")?
                        .to_string(),
                    album: track_info
                        .get("release_title")
                        .and_then(|x| x.as_str())
                        .context("release_title is not a string")?
                        .to_string(),
                    release_id: track_info
                        .get("release_id")
                        .and_then(|x| x.as_number())
                        .context("release_id is not a number")?
                        .to_string(),
                    track_id: track_info
                        .get("id")
                        .context("no id")?
                        .to_string(),
                    genre: track_info
                        .get("genres")
                        .and_then(|x| x.as_array())
                        .context("genre is not an array")?
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    number: track_info
                        .get("position")
                        .and_then(serde_json::Value::as_u64)
                        .context("position is not a number")?
                        .try_into()?,
                    image: track_info
                        .get("image")
                        .and_then(|x| x.get("src"))
                        .and_then(|x| x.as_str())
                        .context("image src is not a string")?
                        .replace("&size={size}&ext=jpg", ""),
                    lyrics: track_info
                        .get("lyrics")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    has_flac,
                },
            );
        }

        Ok(tracks)
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
                if self.quality == Quality::Flac && track_info.has_flac {
                    Quality::Flac
                } else if self.quality == Quality::Flac
                    || self.quality == Quality::MP3High
                {
                    // Fallback from FLAC or if MP3High requested
                    Quality::MP3High
                } else {
                    // Must be MP3Mid requested
                    Quality::MP3Mid
                };

            if effective_quality != self.quality {
                tracing::info!(
                    "Track id {track_id}: Falling back to {} quality (FLAC available: {})",
                    effective_quality,
                    track_info.has_flac
                );
            } else {
                tracing::debug!(
                    "Track id {track_id}: Using requested {} quality (FLAC available: {})",
                    effective_quality,
                    track_info.has_flac
                );
            }

            let response = self
                .http
                .get(ZVUK_DOWNLOAD_URL)
                .query(&[
                    ("quality", &effective_quality.to_string()), // Use determined quality
                    ("id", track_id),
                ])
                .headers(self.default_headers.clone())
                .send()
                .with_context(|| {
                    format!("Failed to download track link for id={track_id}")
                })?;

            let body =
                response.json::<serde_json::Value>().with_context(|| {
                    format!("Failed to parse track link for id={track_id}")
                })?;
            tracing::trace!(
                "{ZVUK_DOWNLOAD_URL} response for id={track_id}: {body:#?}"
            );

            let link = body
                .get("result")
                .and_then(|x| x.get("stream"))
                .and_then(|x| x.as_str())
                .with_context(|| {
                    format!("stream is not a string for id={track_id}")
                })?;

            urls.insert(
                track_id.clone(),
                (link.to_string(), effective_quality),
            ); // Store link and actual quality

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
            .context("Failed to download lyrics")?;
        let body = response
            .json::<serde_json::Value>()
            .context("Failed to parse lyrics")?;
        tracing::trace!("{ZVUK_LYRICS_URL} response: {body:#?}");

        let lyrics = body
            .get("result")
            .and_then(|x| x.get("lyrics"))
            .and_then(|x| x.as_str())
            .context("lyrics is not a string")?
            .to_string();

        let type_ = body
            .get("result")
            .and_then(|x| x.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("lyrics")
            .to_string();

        let lyrics_type = if type_ == "subtitle" {
            LyricsKind::Subtitle
        } else {
            LyricsKind::Lyrics
        };

        Ok(Lyrics {
            kind: lyrics_type,
            text: lyrics,
        })
    }

    fn download_cover(&self, url: &str, path: &Path) -> anyhow::Result<()> {
        if !path.try_exists()? {
            tracing::info!("Downloading cover {}", path.display());
            let response = self.http.get(url).send()?;
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
            .context("Failed to download track")?;
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
            &actual_quality,
        )?;

        Ok(())
    }

    fn write_tags(
        &self,
        filepath: &Path,
        cover_path: &PathBuf,
        track_info: &TrackInfo,
        release_info: &ReleaseInfo,
        actual_quality: &Quality,
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
