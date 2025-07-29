use std::fmt::Display;

use clap::ValueEnum;
use serde::Serialize;

#[derive(ValueEnum, Debug, Clone, Serialize, PartialEq, Eq, Copy)]
pub enum Quality {
    Flac,
    // 320 kbps
    MP3High,
    // 128 kbps
    MP3Mid,
}

impl Quality {
    pub fn extension(self) -> String {
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

pub(super) enum LyricsKind {
    Subtitle,
    Lyrics,
}

#[expect(unused)]
pub(super) struct Lyrics {
    pub(super) kind: LyricsKind,
    pub(super) text: String,
}

#[derive(Debug)]
pub(super) struct ReleaseInfo {
    pub(super) track_ids: Vec<String>,
    pub(super) track_count: u32,
    pub(super) label: String,
    pub(super) date: String,
    pub(super) album: String,
    pub(super) author: String,
}

#[expect(unused)]
#[derive(Debug)]
pub(super) struct TrackInfo {
    pub(super) author: String,
    pub(super) name: String,
    pub(super) album: String,
    pub(super) release_id: String,
    pub(super) track_id: String,
    pub(super) genre: String,
    pub(super) number: u32,
    pub(super) image: String,
    pub(super) lyrics: bool,
    pub(super) has_flac: bool,
}

impl TryFrom<super::models::ZvukRelease> for ReleaseInfo {
    type Error = anyhow::Error;

    fn try_from(
        value: super::models::ZvukRelease,
    ) -> Result<Self, Self::Error> {
        let track_ids: Vec<String> = value
            .track_ids
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let track_count: u32 = track_ids.len().try_into()?;
        Ok(Self {
            track_ids,
            track_count,
            label: value.label_id.to_string(),
            date: value.date.to_string(),
            album: value.title,
            author: value.credits,
        })
    }
}

impl TryFrom<super::models::ZvukTrack> for TrackInfo {
    type Error = anyhow::Error;

    fn try_from(value: super::models::ZvukTrack) -> Result<Self, Self::Error> {
        Ok(Self {
            author: value.credits,
            name: value.title,
            album: value.release_title,
            release_id: value.release_id.to_string(),
            track_id: value.id.to_string(),
            genre: value.genres.join(", "),
            number: value.position.try_into()?,
            image: value.image.src.replace("&size={size}&ext=jpg", ""),
            lyrics: value.lyrics.unwrap_or(false),
            has_flac: value.has_flac,
        })
    }
}

impl TryFrom<super::models::ZvukLyrics> for Lyrics {
    type Error = anyhow::Error;

    fn try_from(
        value: super::models::ZvukLyrics,
    ) -> Result<Self, Self::Error> {
        let lyrics_type = if value.type_ == "subtitle" {
            LyricsKind::Subtitle
        } else {
            LyricsKind::Lyrics
        };

        Ok(Self {
            kind: lyrics_type,
            text: value.lyrics,
        })
    }
}

#[derive(Debug)]
pub(super) struct BookChapter {
    pub(super) author: String,
    pub(super) book_title: String,
    pub(super) title: String,
    pub(super) image: String,
    pub(super) number: u32,
}

impl TryFrom<super::models::ZvukGQLChapter> for BookChapter {
    type Error = anyhow::Error;

    fn try_from(
        value: super::models::ZvukGQLChapter,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            author: value
                .book_authors
                .iter()
                .map(|x| x.rname.clone())
                .collect::<Vec<_>>()
                .join(", "),
            book_title: value.book.title,
            title: value.title,
            image: value.image.src,
            number: value.position.try_into()?,
        })
    }
}
