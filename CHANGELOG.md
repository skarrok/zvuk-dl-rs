# Changelog

<!-- next-header -->

## Unreleased

## v0.4.0

- feat: ✨ add ability to download audio books ([#5](https://github.com/skarrok/zvuk-dl-rs/issues/5))

  URLs like https://zvuk.com/abook/32124448 now work!

- feat: ✨ add timeout option for network requests

  Add `--request-timeout` or `-t` option to configure network timeouts

## v0.3.0

### Added

- feat: ✨ add output directory option by @xzeldon

  Introduce the --output-dir (`-o`) flag to allow specifying the
  download destination directory. Defaults to the current directory.

- feat: ✨ add quality fallback by @xzeldon

  Implement automatic quality fallback: If the requested quality is
  unavailable, the application will now attempt to download the next
  best quality available (FLAC -> MP3High -> MP3Mid).

## v0.2.2

### Fixed

- 🐛 zvuk authorization error ([#2](https://github.com/skarrok/zvuk-dl-rs/issues/2))

    Keep cookies between requests.
    Send user agent with every request. By default it's from latest chrome
    on windows. Can be changed with --user-agent flag.

- 🐛 don't skip lyrics with type `null`

  treat it as `lyrics` type

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
