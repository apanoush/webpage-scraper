use thiserror::Error;
use url::{Url, ParseError};
use reqwest;
use futures::future::join_all;
use scraper::{Html, Selector};
use std::path::Path;

pub struct Video {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub original_src: String,
}

#[derive(Error, Debug)]
pub enum VideosError {
    #[error("UrlError: {0}")]
    UrlError(#[from] ParseError),
    #[error("ReqwestError: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, VideosError>;

impl Video {

    async fn fetch(client: &reqwest::Client, url: Url, original_src: String, idx: usize) -> Result<Self> {
        let response = client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()?;

        let bytes = response.bytes().await?.to_vec();

        let ext = url
            .path_segments()
            .and_then(|s| s.last())
            .and_then(|s| s.split('.').last())
            .unwrap_or("mp4");

        let filename = format!("video_{}.{}", idx, ext);

        Ok(Video {
            bytes,
            filename,
            original_src,
        })
    }

}

#[derive(Default)]
pub struct Videos ( Vec<Video> );

impl Videos {

    pub async fn from(html: String, base_url: String) -> Result<Self> {

        let base_url = Url::parse(&base_url)?;

        let document = Html::parse_document(&html);
        let video_selector = Selector::parse("video[src], video source[src]").unwrap();
        let iframe_selector = Selector::parse("iframe[src]").unwrap();
        let client = Self::init_client()?;

        let mut tasks = Vec::new();
        let mut idx = 0usize;

        for element in document.select(&video_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(res_url) = base_url.join(src) {
                    let task = Video::fetch(&client, res_url, src.to_string(), idx);
                    tasks.push(task);
                    idx += 1;
                }
            }
        }

        for element in document.select(&iframe_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(res_url) = base_url.join(src) {
                    let task = Video::fetch(&client, res_url, src.to_string(), idx);
                    tasks.push(task);
                    idx += 1;
                }
            }
        }

        let results = join_all(tasks).await;

        let videos = results
            .into_iter()
            .filter_map(std::result::Result::ok)
            .collect();

        Ok(Self(videos))
    }

    const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/115.0.0.0 Safari/537.36";

    fn init_client() -> std::result::Result<reqwest::Client, reqwest::Error> {
        reqwest::Client::builder()
            .user_agent(Self::USER_AGENT)
            .build()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn localize_html(&self, html: &str) -> String {
        let mut localized = html.to_string();
        for video in self.0.iter() {
            let replacement = format!("assets/videos/{}", video.filename);
            localized = localized.replace(&video.original_src, &replacement);
        }
        localized
    }

    pub async fn write_to_disk(&self, output_directory: &Path) -> Result<()> {

        if self.len() == 0 {return Ok(());}

        let output_directory = output_directory.join("assets").join("videos");
        std::fs::create_dir_all(&output_directory)?;

        let mut tasks = Vec::new();
        for video in self.0.iter() {
            let output_path = output_directory.join(&video.filename);
            let bytes = video.bytes.clone();
            tasks.push(async move {
                std::fs::write(output_path, bytes)?;
                Ok::<(), std::io::Error>(())
            });
        }

        let results = join_all(tasks).await;

        for res in results {
            res?
        }

        Ok(())
    }

}
