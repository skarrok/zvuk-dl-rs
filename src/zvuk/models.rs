use std::collections::HashMap;

use serde::Deserialize;

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukDownload {
    expire: i64,
    expire_delta: i64,
    pub(super) stream: String,
}

#[derive(Deserialize)]
pub(super) struct ZvukDownloadResponse {
    pub(super) result: ZvukDownload,
}

#[derive(Deserialize)]
pub(super) struct ZvukLyricsResponse {
    pub(super) result: ZvukLyrics,
}

#[derive(Deserialize)]
pub(super) struct ZvukLyrics {
    pub(super) lyrics: String,
    #[serde(alias = "type")]
    pub(super) type_: String,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukImage {
    palette: String,
    palette_bottom: String,
    pub(super) src: String,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukRelease {
    artist_ids: Vec<i64>,
    artist_names: Vec<String>,
    availability: i64,
    pub(super) credits: String,
    pub(super) date: i64,
    explicit: bool,
    genre_ids: Vec<i64>,
    has_image: bool,
    id: i64,
    image: ZvukImage,
    pub(super) label_id: i64,
    search_credits: String,
    search_title: String,
    template: String,
    pub(super) title: String,
    pub(super) track_ids: Vec<i64>,
    #[serde(alias = "type")]
    type_: String,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukTrack {
    artist_ids: Vec<i64>,
    artist_names: Vec<String>,
    availability: i64,
    condition: String,
    pub(super) credits: String,
    duration: i64,
    explicit: bool,
    pub(super) genres: Vec<String>,
    pub(super) has_flac: bool,
    highest_quality: String,
    pub(super) id: i64,
    pub(super) image: ZvukImage,
    pub(super) lyrics: Option<bool>,
    pub(super) position: i64,
    price: i64,
    pub(super) release_id: i64,
    pub(super) release_title: String,
    search_credits: String,
    search_title: String,
    template: String,
    pub(super) title: String,
}

#[derive(Deserialize)]
pub(super) struct ZvukResponse {
    pub(super) result: ZvukResult,
}

#[derive(Deserialize)]
pub(super) struct ZvukResult {
    pub(super) releases: HashMap<String, ZvukRelease>,
    pub(super) tracks: HashMap<String, ZvukTrack>,
}

#[derive(Deserialize)]
pub(super) struct ZvukGQLResponse {
    pub(super) data: ZvukGQLData,
}

#[derive(Deserialize)]
pub(super) struct ZvukGQLData {
    #[serde(alias = "getBooks")]
    pub(super) get_books: Option<Vec<ZvukGQLBook>>,
    #[serde(alias = "mediaContents")]
    pub(super) media_contents: Option<Vec<ZvukGQLMediaContent>>,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukGQLBook {
    pub(super) title: String,
    explicit: bool,
    pub(super) chapters: Vec<ZvukGQLChapter>,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukGQLChapter {
    pub(super) id: String,
    pub(super) title: String,
    availability: i64,
    duration: i64,
    pub(super) image: ZvukGQLImage,
    pub(super) book: ZvukBook,
    #[serde(alias = "bookAuthors")]
    pub(super) book_authors: Vec<ZvukBookAuthor>,
    pub(super) position: i64,
    __typename: String,
}

#[derive(Deserialize)]
pub(super) struct ZvukGQLImage {
    pub(super) src: String,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukBook {
    id: String,
    pub(super) title: String,
    explicit: bool,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukBookAuthor {
    id: String,
    pub(super) rname: String,
    image: ZvukGQLImage,
}

#[derive(Deserialize)]
pub(super) struct ZvukGQLMediaContent {
    __typename: String,
    pub(super) stream: ZvukGQLStream,
}

#[expect(unused)]
#[derive(Deserialize)]
pub(super) struct ZvukGQLStream {
    expire: String,
    pub(super) mid: String,
}
