pub const ZVUK_GQL_GET_BOOK_CHAPTERS_QUERY: &str = "
query getBookChapters($ids: [ID!]!) {
  getBooks(ids: $ids) {
    title
    mark
    explicit
    chapters {
      ...PlayerChapterData
    }
  }
}

fragment PlayerChapterData on Chapter {
  id
  title
  availability
  duration
  childParam
  image {
    src
  }
  book {
    id
    title
    mark
    explicit
  }
  bookAuthors {
    id
    rname
    image {
      src
    }
  }
  position
  __typename
}
";

// quality is auto, hq, hifi, sq
pub const ZVUK_GQL_GET_STREAM: &str = "
query getStream($ids: [ID!]!, $quality: String, $encodeType: String, $includeFlacDrm: Boolean!, $useHLSv2: Boolean!) {
  mediaContents(ids: $ids, quality: $quality, encodeType: $encodeType) {
    ... on Track {
      __typename
      stream {
        expire
        high
        mid
        preview
        flacdrm @include(if: $includeFlacDrm)
      }
      streamV3 @include(if: $useHLSv2) {
        expire
        hls
      }
    }
    ... on Episode {
      __typename
      stream {
        expire
        mid
      }
    }
    ... on Chapter {
      __typename
      stream {
        expire
        mid
      }
    }
  }
}
";

pub const ZVUK_GQL_GET_FULL_TRACK: &str = "
query getFullTrack($ids: [ID!]!, $withReleases: Boolean = false, $withArtists: Boolean = false, $withLikesCount: Boolean = false) {
  getTracks(ids: $ids) {
    id
    title
    position
    duration
    artistTemplate
    explicit
    artistNames
    mark
    zchan
    lyrics
    collectionItemData {
      likesCount @include(if: $withLikesCount)
    }
    genres {
      id
      name
      rname
    }
    artists @include(if: $withArtists) {
      id
      title
      searchTitle
      description
      hasPage
      collectionItemData {
        likesCount
      }
      image {
        src
        palette
        paletteBottom
      }
      secondImage {
        src
        palette
        paletteBottom
      }
      animation {
        artistId
        effect
        image
        background {
          type
          image
          color
          gradient
        }
      }
      mark
    }
    release @include(if: $withReleases) {
      id
      title
      searchTitle
      type
      date
      image {
        src
        palette
        paletteBottom
      }
      genres {
        id
        name
        shortName
      }
      label {
        id
        title
      }
      availability
      artistTemplate
      mark
    }
    hasFlac
  }
}
";

#[expect(dead_code)]
pub const ZVUK_GQL_GET_RELEASES: &str = "
query getReleases($ids: [ID!]!) {
  getReleases(ids: $ids) {
    __typename ...ReleaseGqlFragment
  }
}

fragment ImageInfoGqlFragment on ImageInfo {
  src
  picUrlSmall
  picUrlBig
  palette
  paletteBottom
}

fragment ReleaseGqlFragment on Release {
  artists {
    id
    title
  }
  availability
  date
  explicit
  id
  image {
    __typename
    ...ImageInfoGqlFragment
  }
  label {
    id
  }
  searchTitle
  artistTemplate
  title
  tracks {
    id
  }
  releaseType: type
  collectionItemData {
    likesCount
  }
  zchan
  childParam
  mark
}
";

pub const ZVUK_GQL_GET_PLAYLIST_TRACKS: &str = "
query getPlaylistTracks($id: ID!, $limit: Int = 30, $offset: Int = 0) {
  playlistTracks(id: $id, limit: $limit, offset: $offset) {
    ...PlayerTrackData
  }
}

fragment PlayerTrackData on Track {
  id
  title
  position
  lyrics
  hasFlac
  duration
  explicit
  availability
  artistTemplate
  childParam
  mark
  artists {
    id
    title
    image {
      src
      palette
    }
    mark
  }
  release {
    id
    title
    date
    image {
      src
      palette
    }
  }
  zchan
  __typename
}
";

#[expect(dead_code)]
pub const ZVUK_GQL_GET_PLAYLIST: &str = "
query getPlaylists($ids: [ID!]!) {
  playlists(ids: $ids) {
    __typename ...PlaylistGqlFragment
  }
}

fragment ImageInfoGqlFragment on ImageInfo {
  src picUrlSmall picUrlBig palette paletteBottom
}

fragment TypeInfoGqlFragment on PlaylistTypeInfo {
  type subType
}

fragment generativeMetaFragment on GenerativeMeta {
  title
  description
  generativeCover {
    medium {
      images {
        url
        height
        width
        palette
        paletteBottom
      }
      mask {
        url
        height
        width
        palette
        paletteBottom
      }
      text
    }
  }
}

fragment PlaylistBrandingInfoGqlFragment on Playlist {
  id
  branded
  coverV1 {
    src
  }
  buttons {
    title
    action {
      __typename ... on OpenUrlAction {
        name
        url
        fallbackUrl
        inWebkit
        auth
      }
    }
  }
}

fragment HashtagGqlFragment on Hashtag {
  id
  name
  amount
}

fragment PlaylistGqlFragment on Playlist {
  __typename id title searchTitle updated description image {
    __typename ...ImageInfoGqlFragment
  }
  playlistTracks: tracks {
    id
  }
  typeInfo {
    __typename ...TypeInfoGqlFragment
  }
  generativeMeta {
    __typename ...generativeMetaFragment
  }
  chart {
    trackId
    positionChange
  }
  isPublic
  duration
  userId
  collectionItemData {
    likesCount
  }
  profile {
    name
    image {
      src
    }
  }
  childParam ranked ...PlaylistBrandingInfoGqlFragment hashtags {
    __typename ...HashtagGqlFragment
  }
}
";
