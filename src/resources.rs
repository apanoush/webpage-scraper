use thiserror::Error;
use url::{Url, ParseError};
use reqwest;
use futures::future::{join_all, join};
use scraper::{Html, Selector};
use std::path::Path;
use std::collections::HashSet;
use regex::Regex;

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

    pub async fn from(html: &str, base_url: &str, css_urls: Vec<String>) -> Result<Self> {

        let base_url = Url::parse(base_url)?;

        let mut used_css_names: HashSet<String> = HashSet::new();
        let (html, inline_resources) = extract_inline_styles(html, &mut used_css_names);

        let document = Html::parse_document(&html);
        let css_link_selector = Selector::parse("link[rel=\"stylesheet\"]").unwrap();
        let js_selector = Selector::parse("script[src]").unwrap();
        let client = Self::init_client()?;

        let link_map: Vec<(Url, String)> = document.select(&css_link_selector)
            .filter_map(|el| el.value().attr("href"))
            .filter_map(|href| {
                let fixed = if href.starts_with("//") {
                    format!("https:{}", href)
                } else {
                    href.to_string()
                };
                base_url.join(&fixed).ok().map(|u| (u, href.to_string()))
            })
            .collect();

        let mut seen_urls: HashSet<String> = HashSet::new();
        let mut used_js_names: HashSet<String> = HashSet::new();
        let mut css_tasks = Vec::new();
        let mut js_tasks = Vec::new();

        for css_url_str in css_urls.iter() {
            if let Ok(parsed) = Url::parse(css_url_str) {
                let normalized = parsed.as_str().trim_end_matches('/').to_string();
                if !seen_urls.insert(normalized) { continue; }

                let original_ref = link_map.iter()
                    .find(|(u, _)| *u == parsed)
                    .map(|(_, href)| href.clone())
                    .unwrap_or(css_url_str.clone());
                let filename = unique_resource_name(&parsed, &mut used_css_names, "css");
                let task = Self::download_css(
                    client.clone(), parsed.clone(), original_ref, filename,
                );
                css_tasks.push(task);
            }
        }

        for (resolved_url, original_href) in link_map.iter() {
            let normalized = resolved_url.as_str().trim_end_matches('/').to_string();
            if !seen_urls.insert(normalized) { continue; }

            let filename = unique_resource_name(resolved_url, &mut used_css_names, "css");
            let task = Self::download_css(
                client.clone(), resolved_url.clone(), original_href.clone(), filename,
            );
            css_tasks.push(task);
        }

        for element in document.select(&js_selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(res_url) = base_url.join(src) {
                    let filename = unique_resource_name(&res_url, &mut used_js_names, "js");
                    let task = Self::download_js(
                        client.clone(), res_url, src.to_string(), filename,
                    );
                    js_tasks.push(task);
                }
            }
        }

        let (css_results, js_results) = join(join_all(css_tasks), join_all(js_tasks)).await;

        let mut resources: Vec<Resource> = css_results
            .into_iter()
            .chain(js_results.into_iter())
            .filter_map(std::result::Result::ok)
            .collect();

        resources.extend(inline_resources);

        let nb_css = resources.iter().filter(|r| matches!(r.resource_type, ResourceType::Css)).count();
        let nb_js = resources.iter().filter(|r| matches!(r.resource_type, ResourceType::Js)).count();

        Ok(Self {
            resources,
            nb_css,
            nb_js,
        })
    }

    async fn download_css(
        client: reqwest::Client,
        url: Url,
        original_ref: String,
        filename: String,
    ) -> std::result::Result<Resource, ResourcesError> {
        let mut resource = Resource::fetch(&client, &url).await?;
        resource.resource_type = ResourceType::Css;
        resource.original_ref = original_ref;
        resource.filename = filename;
        Ok(resource)
    }

    async fn download_js(
        client: reqwest::Client,
        url: Url,
        original_ref: String,
        filename: String,
    ) -> std::result::Result<Resource, ResourcesError> {
        let mut resource = Resource::fetch(&client, &url).await?;
        resource.resource_type = ResourceType::Js;
        resource.original_ref = original_ref;
        resource.filename = filename;
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
        if self.nb_css > 0 {
            std::fs::create_dir_all(&css_dir)?;
        }
        if self.nb_js > 0 {
            std::fs::create_dir_all(&js_dir)?;
        }

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

fn filename_from_url(url: &Url) -> String {
    url.path_segments()
        .and_then(|s| s.last())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "style.css".to_string())
}

fn unique_resource_name(url: &Url, used: &mut HashSet<String>, ext: &str) -> String {
    let base = filename_from_url(url);
    let base = if base.contains('.') { base } else { format!("{}.{}", base, ext) };
    if used.insert(base.clone()) {
        return base;
    }
    let stem = match base.rfind('.') {
        Some(dot) => base[..dot].to_string(),
        None => base,
    };
    for i in 1.. {
        let candidate = format!("{}_{}.{}", stem, i, ext);
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}

fn extract_inline_styles(html: &str, used_css_names: &mut HashSet<String>) -> (String, Vec<Resource>) {
    let re = Regex::new(r"(?is)<style[^>]*>(.*?)</style>").unwrap();
    let mut modified = String::with_capacity(html.len());
    let mut resources = Vec::new();
    let mut last_end = 0;

    for caps in re.captures_iter(html) {
        if let Some(m) = caps.get(0) {
            let content = caps.get(1).map(|c| c.as_str()).unwrap_or("");
            let start = m.start();
            let end = m.end();

            let filename = next_inline_name(used_css_names);
            let replacement = format!("<link rel=\"stylesheet\" href=\"assets/css/{}\">", filename);

            modified.push_str(&html[last_end..start]);
            modified.push_str(&replacement);
            last_end = end;

            resources.push(Resource {
                bytes: content.as_bytes().to_vec(),
                filename,
                original_ref: String::new(),
                resource_type: ResourceType::Css,
            });
        }
    }

    modified.push_str(&html[last_end..]);
    (modified, resources)
}

fn next_inline_name(used: &mut HashSet<String>) -> String {
    for i in 0.. {
        let name = format!("inline_{}.css", i);
        if used.insert(name.clone()) {
            return name;
        }
    }
    unreachable!()
}
