use thiserror::Error;
use url::{Url, ParseError};
use reqwest;
use futures::future::join_all;
use scraper::{Html, Selector};
use std::path::Path;

pub enum ResourceType {
    Css,
    Js,
}

pub struct Resource {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub original_ref: String,
    pub resource_type: ResourceType,
}

#[derive(Error, Debug)]
pub enum ResourcesError {
    #[error("UrlError: {0}")]
    UrlError(#[from] ParseError),
    #[error("ReqwestError: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ResourcesError>;

impl Resource {

    async fn fetch(client: &reqwest::Client, url: &Url) -> Result<Self> {
        let response = client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()?;

        let bytes = response.bytes().await?.to_vec();

        Ok(Resource {
            bytes,
            filename: String::new(),
            original_ref: String::new(),
            resource_type: ResourceType::Css,
        })
    }

}

pub struct Resources {
    resources: Vec<Resource>,
    nb_css: usize,
    nb_js: usize,
}

impl Resources {

    pub async fn from(html: &str, base_url: &str) -> Result<Self> {

        let base_url = Url::parse(base_url)?;

        let document = Html::parse_document(html);
        let css_selector = Selector::parse("link[rel=\"stylesheet\"]").unwrap();
        let js_selector = Selector::parse("script[src]").unwrap();
        let client = Self::init_client()?;

        let mut tasks = Vec::new();
        let mut css_count = 0usize;
        let mut js_count = 0usize;

        for element in document.select(&css_selector) {
            if let Some(href) = element.value().attr("href") {
                if let Ok(res_url) = base_url.join(href) {
                    css_count += 1;
                    let idx = css_count - 1;
                    let task = Self::download_with_meta(
                        client.clone(), res_url, href.to_string(),
                        ResourceType::Css, idx,
                    );
                    tasks.push(task);
                }
            }
        }

        for element in document.select(&js_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(res_url) = base_url.join(src) {
                    js_count += 1;
                    let idx = js_count - 1;
                    let task = Self::download_with_meta(
                        client.clone(), res_url, src.to_string(),
                        ResourceType::Js, idx,
                    );
                    tasks.push(task);
                }
            }
        }

        let results = join_all(tasks).await;

        let resources: Vec<Resource> = results
            .into_iter()
            .filter_map(std::result::Result::ok)
            .collect();

        Ok(Self {
            resources,
            nb_css: css_count,
            nb_js: js_count,
        })
    }

    async fn download_with_meta(
        client: reqwest::Client,
        url: Url,
        original_ref: String,
        resource_type: ResourceType,
        idx: usize,
    ) -> std::result::Result<Resource, ResourcesError> {
        let mut resource = Resource::fetch(&client, &url).await?;
        resource.resource_type = resource_type;
        resource.original_ref = original_ref;
        resource.filename = match resource.resource_type {
            ResourceType::Css => format!("style_{}.css", idx),
            ResourceType::Js => format!("script_{}.js", idx),
        };
        Ok(resource)
    }

    const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/115.0.0.0 Safari/537.36";

    fn init_client() -> std::result::Result<reqwest::Client, reqwest::Error> {
        reqwest::Client::builder()
            .user_agent(Self::USER_AGENT)
            .build()
    }

    pub fn len(&self) -> usize {
        self.resources.len()
    }

    pub fn nb_css(&self) -> usize {
        self.nb_css
    }

    pub fn nb_js(&self) -> usize {
        self.nb_js
    }

    pub fn localize_html(&self, html: &str) -> String {
        let mut localized = html.to_string();
        for resource in self.resources.iter() {
            let subdir = match resource.resource_type {
                ResourceType::Css => "css",
                ResourceType::Js => "js",
            };
            let replacement = format!("assets/{}/{}", subdir, resource.filename);
            localized = localized.replace(&resource.original_ref, &replacement);
        }
        localized
    }

    pub async fn write_to_disk(&self, output_directory: &Path) -> Result<()> {

        if self.len() == 0 {return Ok(());}

        let assets_dir = output_directory.join("assets");
        let css_dir = assets_dir.join("css");
        let js_dir = assets_dir.join("js");
        std::fs::create_dir_all(&css_dir)?;
        std::fs::create_dir_all(&js_dir)?;

        let mut tasks = Vec::new();
        for resource in self.resources.iter() {
            let subdir = match resource.resource_type {
                ResourceType::Css => &css_dir,
                ResourceType::Js => &js_dir,
            };
            let output_path = subdir.join(&resource.filename);
            let bytes = resource.bytes.clone();
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
