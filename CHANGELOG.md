# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added

- ✨ Add parallel track download

  Download tracks and audiobook chapters concurrently with configurable
  worker count via `--parallel` option. Default is set to 1.

- ✨ Download playlist tracks ([#8](https://github.com/skarrok/zvuk-dl-rs/issues/8))

  URLs like https://zvuk.com/playlist/8081036 now work!
  Tracks from playlists are saved without album subdirectories and covers saved next to each track.

## [0.4.3] - 2026-03-07

### Fixed

- 🐛 parsing track metadata ([#7](https://github.com/skarrok/zvuk-dl-rs/issues/7))

  mark ZvukGQLArtist.description as optional

## [0.4.2] - 2026-03-07

### Fixed

- 🐛 parsing track metadata ([#7](https://github.com/skarrok/zvuk-dl-rs/issues/7))

## [0.4.1] - 2026-03-07

### Fixed

- 🐛 support getting track metadata via graphql

## [0.4.0] - 2025-08-16

### Added

- ✨ Add ability to download audio books ([#5](https://github.com/skarrok/zvuk-dl-rs/issues/5))

  URLs like https://zvuk.com/abook/32124448 now work!

- ✨ Add timeout option for network requests

  Add `--request-timeout` or `-t` option to configure network timeouts

## [0.3.0] - 2025-04-06

### Added

- ✨ Add output directory option by @xzeldon

  Introduce the --output-dir (`-o`) flag to allow specifying the
  download destination directory. Defaults to the current directory.

- ✨ Add quality fallback by @xzeldon

  Implement automatic quality fallback: If the requested quality is
  unavailable, the application will now attempt to download the next
  best quality available (FLAC -> MP3High -> MP3Mid).

## [0.2.2] - 2024-11-29

### Fixed

- 🐛 zvuk authorization error ([#2](https://github.com/skarrok/zvuk-dl-rs/issues/2))

    Keep cookies between requests.
    Send user agent with every request. By default it's from latest chrome
    on windows. Can be changed with --user-agent flag.

- 🐛 don't skip lyrics with type `null`

  treat it as `lyrics` type

## [0.2.1] - 2024-10-14

### Fixed

- 🐛 sanitize folder and file path ([#1](https://github.com/skarrok/zvuk-dl-rs/issues/1))

  replace reserved characters with underscores `_`

  reserved characters:
  - on windows: `<`, `>`, `:`, `"`, `/`, `\\`, `|`, `?`, `*`
  - on unix: `/`

## [0.2.0] - 2024-09-29

### Added

- ✨ Add support for grabbing tracks in MP3 format

  With command line argument `-q` is it now possible to select `mp3-high`
  or `mp3-mid` quality.
  Default is `flac`. Usually `mp3-high` has 320 kbps bitrate and `mp3-mid`
  is 128 kbps.

## [0.1.0] - 2024-09-22

🎉 Initial release

<!-- next-url -->
[Unreleased]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.4.3...HEAD
[0.4.3]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/skarrok/zvuk-dl-rs/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/skarrok/zvuk-dl-rs/releases/tag/v0.1.0
