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
query getStream($ids: [ID!]!, $quality: String, $encodeType: String, $includeFlacDrm: Boolean!) {
  mediaContents(ids: $ids, quality: $quality, encodeType: $encodeType) {
    ... on Track {
      __typename
      stream {
        expire
        high
        mid
        flacdrm @include(if: $includeFlacDrm)
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
