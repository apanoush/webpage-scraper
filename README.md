# `wbps` — webpage scraper

CLI tool that scrapes a webpage: HTML, PDF, Markdown, metadata JSON, images, CSS, JavaScript, and optionally videos.

## Architecture

```
main.rs → Browser → (headless Chrome + 25s nav timeout + scroll + DOM stability wait + cookie popup removal)
                → WebPage::from_tab()
                      ├── PDF capture (early, via tab.print_to_pdf)
                      ├── Pandoc (HTML → Markdown, skipped with --no-conversions)
                      ├── Images (download img src, srcset, data-src, data-srcset,
                      │           picture source srcset, base64 inline, dedup)
                      ├── Resources (inline <style>/<script> extraction,
                      │           collect CSS via performance API + <link> fallback,
                      │           original filenames for @import support, download <script src>)
                      └── Videos (download <video src>, <source src>, <iframe src> -- opt-in)
                → WebPage::write_to_disk()
                      ├── <Title>.html (title sanitized, assets localised, charset → utf-8)
                      ├── metadata.json
                      ├── conversions/  (PDF + Markdown)
                      └── assets/  (css/ + js/ + images/ + videos/)
```

![Overview](assets/overview.pdf)

### Navigation phases

Each phase has a cap to prevent hanging on broken or slow pages:

| Phase | Cap | Max time |
|---|---|---|
| Initial settle | fixed 2s | 2s |
| URL polling | 20 iterations every 500ms | 10s |
| Scroll to bottom | 40 cycles, 500px/100ms | 8s |
| DOM stability wait | 10 cycles every 1s | 10s |
| **Total** | **outer timeout** | **25s** |

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
| `--download-videos` | Also download videos from `<video>`, `<source>`, and `<iframe>` elements |

## Output structure

```
<output_dir>/
├── <Title>.html
├── metadata.json
├── conversions/
│   ├── <Title>.md
│   └── <Title>.pdf
└── assets/
    ├── css/
    │   ├── style.css
    │   └── inline_0.css
    ├── js/
    │   ├── script.js
    │   └── inline_0.js
    └── images/
        └── image.jpg
    └── videos/           (only with --download-videos)
        └── video_0.mp4
```
