use std::{fs, path::{Path, PathBuf}};
use pandoc;
use time::OffsetDateTime;
use thiserror::Error;
use std::sync::Arc;
use headless_chrome;
use anyhow;
use futures::future;
use serde_json;
use serde::Serialize;
use crate::images::{Images, ImagesError};
use crate::resources::{Resources, ResourcesError};
use crate::videos::{Videos, VideosError};
use regex::Regex;
use reqwest;

pub struct WebPage {
    pub url: String,
    pub title: String,
    html: String,
    images: Images,
    resources: Resources,
    videos: Videos,
    markdown: String,
    pdf: Option<Vec<u8>>,
    info_json: InfoJson
}

#[derive(Serialize)]
pub struct InfoJson {
    url: String,
    title: String,
    date: String,
    nb_images: usize,
    nb_css: usize,
    nb_js: usize,
    nb_videos: usize,
}

#[derive(Error, Debug)]
pub enum WebPageError {
    #[error("I/O Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("MarkdownConversion error: {0}")]
    MarkdownConversionError(#[from] pandoc::PandocError),
    #[error("Task failed: {0}")]
    TaskFailed(#[from] tokio::task::JoinError),
    #[error("Time error: {0}")]
    TimeError(#[from] time::error::IndeterminateOffset),
    #[error("ImagesError: {0}")]
    ImagesError(#[from] ImagesError),
    #[error("ResourcesError: {0}")]
    ResourcesError(#[from] ResourcesError),
    #[error("VideosError: {0}")]
    VideosError(#[from] VideosError),
    #[error("AnyhowError: {0}")]
    AnyhowError(#[from] anyhow::Error),
    #[error("JSON conversion error: {0}")]
    JsonConversionError(#[from] serde_json::Error)
}

pub type Result<T> = std::result::Result<T, WebPageError>;

impl WebPage {

    pub async fn from_tab(tab: Arc<headless_chrome::Tab>, no_conversions: bool, download_videos: bool, css_urls: Vec<String>) -> Result<Self> {

        let today = OffsetDateTime::now_local()?.date().to_string();

        let title = tab.get_title()?;
        let url = tab.get_url();
        let html = tab.get_content()?;

        let pdf = if no_conversions {
            None
        } else {
            Some(tab.print_to_pdf(None)?)
        };

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/115.0.0.0 Safari/537.36")
            .build()
            .map_err(|e| WebPageError::ImagesError(ImagesError::ReqwestError(e)))?;

        let images_fut = Images::from(&html, &url, &client);
        let resources_fut = Resources::from(&html, &url, css_urls, &client);
        let videos_fut = if download_videos {
            Some(Videos::from(html.clone(), url.clone(), &client))
        } else {
            None
        };

        let (markdown, images, resources, videos);

        match (no_conversions, download_videos) {
            (true, true) => {
                let (i, r, v) = future::join3(images_fut, resources_fut, videos_fut.unwrap()).await;
                images = i?; resources = r?; videos = v?;
                markdown = String::new();
            }
            (true, false) => {
                let (i, r) = future::join(images_fut, resources_fut).await;
                images = i?; resources = r?;
                videos = Videos::default();
                markdown = String::new();
            }
            (false, true) => {
                let md = WebPage::html2md(html.clone());
                let (md, i, r, v) = future::join4(md, images_fut, resources_fut, videos_fut.unwrap()).await;
                markdown = md?; images = i?; resources = r?; videos = v?;
            }
            (false, false) => {
                let md = WebPage::html2md(html.clone());
                let (md, i, r) = future::join3(md, images_fut, resources_fut).await;
                markdown = md?; images = i?; resources = r?;
                videos = Videos::default();
            }
        }

        let nb_images = images.len();

        let info_json = InfoJson {
            url: url.clone(), title: title.clone(), date: today.clone(),
            nb_images,
            nb_css: resources.nb_css(), nb_js: resources.nb_js(),
            nb_videos: videos.len(),
        };

        Ok( Self {
            url,
            title,
            markdown,
            images,
            resources,
            videos,
            pdf,
            html,
            info_json
        })


    }

    async fn html2md(html: String) -> Result<String> {
        tokio::task::spawn_blocking(move || {
            let mut pandoc = pandoc::Pandoc::new();
            pandoc
                .set_input(pandoc::InputKind::Pipe(html))
                .set_input_format(pandoc::InputFormat::Html, vec![])
                .set_output(pandoc::OutputKind::Pipe)
                .set_output_format(pandoc::OutputFormat::Other("gfm-raw_html".to_string()), vec![]);
            let res = pandoc.execute()?;
            match res {
                pandoc::PandocOutput::ToBuffer(e) => Ok(e),
                _ => Err(WebPageError::MarkdownConversionError(
                    pandoc::PandocError::PandocNotFound
                ))
            }
        }).await?
    }

    pub async fn write_to_disk(&self, output_path: &str, no_conversions: bool) -> Result<()> {

        let output_path = PathBuf::from(output_path);

        if output_path.is_file() || output_path.is_dir() {
            return Err(WebPageError::IO(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Output path already exists")
            ));
        }

        std::fs::create_dir(&output_path)?;

        let html_res = self.output_html(output_path.as_path());
        let images_res = self.images.write_images_to_disk(output_path.as_path());
        let resources_res = self.resources.write_to_disk(output_path.as_path());
        let videos_res = self.videos.write_to_disk(output_path.as_path());
        let info_json_res = self.output_info_json(output_path.as_path());

        if no_conversions {
            let (html_res, images_res, resources_res, videos_res) = future::join4(html_res, images_res, resources_res, videos_res).await;
            let info_json_res = info_json_res.await;
            html_res?; images_res?; resources_res?; videos_res?; info_json_res?;
        } else {
            std::fs::create_dir(output_path.join("conversions"))?;
            let pdf_res = self.output_pdf(output_path.as_path());
            let md_res = self.output_markdown(output_path.as_path());
            let (html_res, pdf_res, md_res, images_res, resources_res) = future::join5(html_res, pdf_res, md_res, images_res, resources_res).await;
            let videos_res = videos_res.await;
            let info_json_res = info_json_res.await;
            html_res?; pdf_res?; md_res?; images_res?; resources_res?; videos_res?; info_json_res?;
        }

        Ok(())
    }

    pub async fn output_pdf(&self, output_path: &Path) -> Result<()> {
        let output_path = output_path.join("conversions").join(format!("{}.pdf", safe_title(&self.title)));
        if let Some(ref pdf_bytes) = self.pdf {
            std::fs::write(output_path, pdf_bytes)?;
        }
        Ok(())
    }

    async fn output_html(&self, output_path: &Path) -> Result<()> {
        let html_path = output_path.join(format!("{}.html", safe_title(&self.title)));
        //let html_path = output_path.join("index.html");
        let localized_html = self.images.localize_html(&self.html);
        let localized_html = self.resources.localize_html(&localized_html);
        let localized_html = self.videos.localize_html(&localized_html);
        let localized_html = fix_charset(&localized_html);
        fs::write(html_path, localized_html)?;
        Ok(())
    }

    async fn output_markdown(&self, output_path: &Path) -> Result<()> {
        let output_path = output_path.join("conversions").join(format!("{}.md", safe_title(&self.title)));
        fs::write(output_path, &self.markdown)?;
        Ok(())
    }
     
    async fn output_info_json(&self, output_path: &Path) -> Result<()> {
        let output_path = output_path.join("metadata.json");
        let json = serde_json::to_string_pretty(&self.info_json)?;
        fs::write(output_path, json)?;
        Ok(())
    }

}

pub fn safe_title(title: &str) -> String {
    title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-")
}

fn fix_charset(html: &str) -> String {
    let re = Regex::new(r#"(?i)(<meta[^>]*charset\s*=\s*["'])([^"']+)(["'])"#).unwrap();
    let html = re.replace_all(html, "${1}utf-8$3").to_string();
    let re2 = Regex::new(r"(?i)(<meta[^>]*charset\s*=\s*)([a-zA-Z0-9_-]+)").unwrap();
    re2.replace_all(&html, "${1}utf-8").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_html_epfl() {
        
        let html = std::fs::read_to_string("test/htmls/EPFL.html").unwrap();
        let md = WebPage::html2md(html).await.unwrap();
        //let md = WebPage::html_to_simple_markdown(&html);
        std::fs::write("test/test_markdown/markdown_epfl.md", md).unwrap();
        
    }

    #[tokio::test]
    async fn test_html_ecal() {
        
        let html = std::fs::read_to_string("test/htmls/100 BESTE PLAKATE 24, 17.12.2025–15.01.2026, Galerie l'elac, ECAL - ECAL.html").unwrap();
        let md = WebPage::html2md(html).await.unwrap();
        //let md = WebPage::html_to_simple_markdown(&html);
        std::fs::write("test/test_markdown/markdown_ecal.md", md).unwrap();
        
    }


}
