# zvuk-dl ![Release](https://github.com/skarrok/zvuk-dl-rs/actions/workflows/release.yml/badge.svg)

Download albums and tracks in high quality (FLAC or MP3) from [zvuk.com](https://zvuk.com)

> [!IMPORTANT]
> You must have zvuk.com account and paid subscription to use this tool.

## Quickstart

```sh
# write you token to config
echo "TOKEN=YOUR_TOKEN" > .env

# or provide it as an environment variable
export TOKEN=YOUR_TOKEN

# or provide it as an argument to command
# zvuk-dl --token YOUR_TOKEN ...

# download track and album
zvuk-dl https://zvuk.com/track/128672726 https://zvuk.com/release/29970563
```

Tracks are downloaded to current directory with
`Author - Album (Year)/## - Title.flac` format and tags are added
automatically.

By default, zvuk-dl downloads and embeds lyrics and downloads album cover.
You can enable cover embedding with `--embed-cover` option.
Album cover is resized to be less than 1MB using imagemagick.

> [!WARNING]
> If you don't have [imagemagick](https://imagemagick.org) installed, disable
cover resizing with `--resize-cover=false` or command will fail.

## Getting your personal token

Token looks like hexadecimal string with 32 symbols.
Simplest way to get it is to visit zvuk.com and log in.
Make sure you have paid subscription.
Open your browser's developer tools and view cookies for `https://zvuk.com` domain.
Your token will be in there under `auth` name.

For example in Chrome:

1. Click the Three-dot menu button to the right of the address bar and select
More Tools > Developer Tools.
2. In the top bar select Application tab.
3. In the left sidebar under Storage -> Cookies select `https://zvuk.com`
4. In the right pane select `auth` cookie and copy it.
5. Write it to `.env` file in the current directory with
`echo TOKEN=YOUR_TOKEN > .env`

## Configuration

You can pass configuration parameters as command line arguments or environment
variables or write it to `.env` file in the current directory.

```txt
Download albums and tracks in high quality (FLAC) from Zvuk.com

Usage: zvuk-dl [OPTIONS] --token <TOKEN> <URLS>...

Arguments:
  <URLS>...
          URLs of releases or tracks

          URLs must look like https://zvuk.com/track/128672726 or https://zvuk.com/release/29970563

Options:
      --token <TOKEN>
          Zvuk Token

          [env: TOKEN]

  -q, --quality <QUALITY>
          Quality of tracks to grab

          [env: QUALITY=]
          [default: flac]
          [possible values: flac, mp3-high, mp3-mid]

      --embed-cover[=<EMBED_COVER>]
          Embed album cover into tracks

          [env: EMBED_COVER=]
          [default: false]
          [possible values: true, false]

      --resize-cover[=<RESIZE_COVER>]
          Resize album cover

          [env: RESIZE_COVER=]
          [default: true]
          [possible values: true, false]

      --resize-cover-limit <RESIZE_COVER_LIMIT>
          Resize if cover size in bytes bigger than this value

          [env: RESIZE_COVER_LIMIT=]
          [default: 2000000]

      --download-lyrics[=<DOWNLOAD_LYRICS>]
          Download and embed lyrics

          [env: DOWNLOAD_LYRICS=]
          [default: true]
          [possible values: true, false]

      --resize-command <RESIZE_COMMAND>
          Resize cover command. By default uses imagemagick

          [env: RESIZE_COMMAND=]
          [default: "magick {source} -define jpeg:extent=1MB {target}"]

      --log-level <LOG_LEVEL>
          Verbosity of logging

          [env: LOG_LEVEL=]
          [default: debug]
          [possible values: off, trace, debug, info, warn, error]

      --log-format <LOG_FORMAT>
          Format of logs

          [env: LOG_FORMAT=]
          [default: console]

          Possible values:
          - console: Pretty logs for debugging
          - json:    JSON logs

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Building

It is as simple as cloning this repository and running

```bash
cargo build --release
```
