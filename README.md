# `wbps` — webpage scraper

CLI tool that scrapes a webpage: HTML, PDF, Markdown, metadata JSON, images, CSS, JavaScript, and optionally videos.

## Architecture

```
main.rs → Browser → (headless Chrome + scroll + capture)
                → WebPage::from_tab()
                      ├── Pandoc (HTML → Markdown)
                      ├── Images (download <img>, data-srcset, base64 inline)
                      ├── Resources (download <link rel="stylesheet">, <script src>)
                      └── Videos (download <video src>, <source src> -- opt-in)
                → WebPage::write_to_disk()
                      ├── index.html (with localised asset references)
                      ├── metadata.json
                      ├── conversions/  (PDF + Markdown)
                      └── assets/  (css/ + js/ + images/ + videos/)
```

![Overview](assets/overview.pdf)

## Dependencies

- [pandoc](https://pandoc.org/) — HTML-to-Markdown conversion (not needed with `--no-conversions`)
- Chrome / Chromium — headless browser via `headless_chrome` crate

## Usage

```
wbps <URL> [OUTPUT_DIRECTORY] [--no-conversions] [--download-videos]
```

| Argument | Description |
|---|---|
| `URL` | Webpage to scrape |
| `OUTPUT_DIRECTORY` | Output folder name (defaults to page title) |
| `--no-conversions` | Skip PDF, Markdown, and Pandoc invocation |
| `--download-videos` | Also download videos from `<video>` and `<source>` elements |

## Output structure

```
<output_dir>/
├── index.html
├── metadata.json
├── conversions/
│   ├── <Title>.md
│   └── <Title>.pdf
└── assets/
    ├── css/
    │   └── style_0.css
    ├── js/
    │   └── script_0.js
    └── images/
        └── image.jpg
    └── videos/           (only with --download-videos)
        └── video_0.mp4
```
