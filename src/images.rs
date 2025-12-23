use thiserror::Error;
use url::{Url, ParseError};
use reqwest;
use base64::Engine;
use futures::future::join_all;
use scraper::{Html, Selector};
use std::path::Path;

pub struct Image {
    pub image_bytes: Vec<u8>,
    pub filename: String,
}

#[derive(Error, Debug)]
pub enum ImagesError {
    #[error("UrlError: {0}")]
    UrlError(#[from] ParseError),
    #[error("ReqwestError: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Base64Error when parsing image: {0}")]
    Base64Error(#[from] base64::DecodeError),
    #[error("Base64CommaError")]
    Base24CommaError,
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("srcset error")]
    SrcsetError
}

pub type Result<T> = std::result::Result<T, ImagesError>;

impl Image {

    async fn handle_image_src(src: &str, base_url: &Url, client: &reqwest::Client) -> Result<Self> {
        // Case 1: data:image/...;base64,...
        if src.starts_with("data:image") {
            return Self::parse_data_url(src);
        }

        // Case 2: normal URL or relative URL
        let img_url = base_url
            .join(src)?;

        Image::fetch_image(client, &img_url).await
    }

    async fn handle_image_srcset(srcset: &str, client: &reqwest::Client) -> Result<Self> {
        
        let img_url = Image::extract_last_image_url(srcset).ok_or(ImagesError::SrcsetError)?;
        let img_url = Url::parse(img_url)?;

        Image::fetch_image(client, &img_url).await
    }

    async fn fetch_image(client: &reqwest::Client, img_url: &Url) -> Result<Self> {

        let response = client
            .get(img_url.clone())
            .send()
            .await?
            .error_for_status()?;

        let bytes = response.bytes().await?.to_vec();

        let filename = img_url
            .path_segments()
            .and_then(|s| s.last())
            .filter(|s| !s.is_empty())
            .unwrap_or("image");

        Ok(Image {
            image_bytes: bytes,
            filename: filename.to_string(),
        })
    }

    fn extract_last_image_url(srcset: &str) -> Option<&str> {
        srcset
            .split(',')
            .map(|s| s.trim())
            .filter_map(|entry| entry.split_whitespace().next())
            .filter(|url| {
                url.ends_with(".jpg")
                    || url.ends_with(".jpeg")
                    || url.ends_with(".png")
                    || url.ends_with(".webp")
            })
            .last()
    }

    fn parse_data_url(src: &str) -> Result<Self> {
        // example:
        // data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA...

        let (meta, data) = src
            .split_once(',').ok_or(ImagesError::Base24CommaError)?;

        let extension = meta
            .split(';')
            .next()
            .and_then(|m| m.split('/').nth(1))
            .unwrap_or("img");

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)?;

        Ok(Image {
            image_bytes: bytes,
            filename: format!("inline.{}", extension),
        })
    }

    async fn write_to_disk(&self, directory: &Path) -> Result<()> {
        let output_path = directory.join(&self.filename);
        std::fs::write(output_path, &self.image_bytes)?;
        Ok(())
    }

}


pub struct Images ( pub Vec<Image> );

impl Images {
    
    pub async fn from(html: &str, base_url: &str) -> Result<Self> {

        let base_url = Url::parse(base_url)?;

        let document = Html::parse_document(html);
        let img_selector = Selector::parse("img").unwrap();
        //let client = Client::new();
        let client = Self::init_client()?;

        let mut tasks_src = Vec::new();
        let mut tasks_srcset = Vec::new();

        for element in document.select(&img_selector) {
            if let Some(src) = element.value().attr("src") {
                // Spawn async task per image
                let task = Image::handle_image_src(src, &base_url, &client);

                tasks_src.push(task);
            }

            if let Some(srcset) = element.attr("data-srcset") {
                let task = Image::handle_image_srcset(srcset, &client);
                tasks_srcset.push(task);
            }
        }

        // Run all downloads concurrently
        let results_src = join_all(tasks_src).await;
        let results_srcset = join_all(tasks_srcset).await;

        // Collect successful images only
        let images = results_src
            .into_iter()
            .chain(results_srcset.into_iter())
            .filter_map(Result::ok)
            .collect();

        Ok(Self(images))
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

    pub async fn write_images_to_disk(&self, output_directory: &Path) -> Result<()> {

        if self.len() == 0 {return Ok(());}
        
        let output_directory = output_directory.join("images");
        std::fs::create_dir(&output_directory)?;


        let mut tasks = Vec::new();
        for image in self.0.iter() {
            let task = image.write_to_disk(&output_directory);
            tasks.push(task);
        }

        let results = join_all(tasks).await;

        for res in results {
            res?
        }

        Ok(())

    }
    
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn al_images_from_website() {
        
        let html: String = std::fs::read_to_string("test/htmls/EPFL.html").unwrap();
        
        let base_url = "https://www.epfl.ch/en/";
        let output_path = "test/test_images_epfl";
        let images = Images::from(&html, base_url).await.unwrap();
        images.write_images_to_disk(Path::new(output_path)).await.unwrap();
    }
}
