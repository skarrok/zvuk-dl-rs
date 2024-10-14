# Changelog

## Unreleased

## v0.2.1

### Fixed

- 🐛 sanitize folder and file path ([#1](https://github.com/skarrok/zvuk-dl-rs/issues/1))

  replace reserved characters with underscores `_`

  reserved characters:
  - on windows: `<`, `>`, `:`, `"`, `/`, `\\`, `|`, `?`, `*`
  - on unix: `/`

## v0.2.0

### Added

- ✨ Add support for grabbing tracks in MP3 format

  With command line argument `-q` is it now possible to select `mp3-high`
  or `mp3-mid` quality.
  Default is `flac`. Usually `mp3-high` has 320 kbps bitrate and `mp3-mid`
  is 128 kbps.

## v0.1.0

🎉 Initial release
