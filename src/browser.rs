use headless_chrome;
use anyhow;
use url::{Url, ParseError};
use thiserror::Error;
use crate::webpage::{WebPage, WebPageError, safe_title};
use std::path::Path;
use std::sync::Arc;
use std::time;
use serde_json;
use tokio;

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("ChromeError: {0}")]
    ChromeError(#[from] anyhow::Error),
    #[error("UrlError, can't parse given URL: {0}")]
    UrlError(#[from] ParseError),
    #[error("WebPageError: {0}")]
    WebPageError(#[from] WebPageError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error)
}
pub type Result<T> = std::result::Result<T, BrowserError>;

pub struct Browser (headless_chrome::Browser);

impl Browser {
    
    pub fn new() -> Result<Self> {
        Ok(Self(headless_chrome::Browser::default()?))
    }

    fn open_tab_impl(url: &str, browser: &headless_chrome::Browser) -> Result<Arc<headless_chrome::Tab>> {
        Url::parse(url)?;
        let tab = browser.new_tab()?;
        tab.navigate_to(url)?.wait_until_navigated()?;
        Ok(tab)
    }

    fn post_nav_setup(tab: &Arc<headless_chrome::Tab>) -> Result<()> {
        tab.evaluate(
            "new Promise((resolve) => {
                const getH = () => Math.max(
                    document.body.scrollHeight,
                    document.body.offsetHeight,
                    document.documentElement.scrollHeight,
                    document.documentElement.offsetHeight
                );
                let lastH = getH();
                let stable = 0;
                const maxCycles = 100;
                let cycles = 0;
                const timer = setInterval(() => {
                    window.scrollBy(0, 300);
                    cycles++;
                    setTimeout(() => {
                        const h = getH();
                        const atBottom = window.innerHeight + window.scrollY >= h - 50;
                        if (h === lastH && atBottom) { stable++; } else { stable = 0; lastH = h; }
                        if ((stable >= 3 && atBottom) || cycles >= maxCycles) {
                            clearInterval(timer);
                            window.scrollTo(0, 0);
                            resolve();
                        }
                    }, 150);
                }, 250);
            })",
            true,
        )?;

        tab.evaluate(
            "new Promise((resolve) => {
                let lastLength = document.body.innerHTML.length;
                let stable = 0;
                const neededStable = document.querySelector('.loader, .upt-loader, .loading, [data-loading], [aria-busy=\"true\"], .spinner') ? 3 : 2;
                const check = setInterval(() => {
                    const len = document.body.innerHTML.length;
                    const busy = document.querySelector('.loader, .upt-loader, .loading, [data-loading], [aria-busy=\"true\"], .spinner');
                    if (len === lastLength && !busy) {
                        stable++;
                        if (stable >= neededStable) {
                            clearInterval(check);
                            resolve();
                        }
                    } else {
                        stable = 0;
                        lastLength = len;
                    }
                }, 1000);
            })",
            true,
        )?;

        tab.evaluate(
            "document.querySelectorAll(
                '[class*=\"cookie\"],[id*=\"cookie\"],[class*=\"consent\"],[id*=\"consent\"],
                 [class*=\"popup\"],[id*=\"popup\"],[class*=\"modal\"],[id*=\"modal\"],
                 [class*=\"overlay\"],[id*=\"overlay\"],[class*=\"banner\"],[id*=\"banner\"]'
            ).forEach(el => el.remove())",
            false,
        )?;

        Ok(())
    }

    pub async fn open_tab(&self, url: &str, no_conversions: bool, download_videos: bool) -> Result<WebPage> {
    
        let url_owned = url.to_string();
        let browser = self.0.clone();

        let tab = tokio::time::timeout(
            time::Duration::from_secs(20),
            tokio::task::spawn_blocking(move || {
                Self::open_tab_impl(&url_owned, &browser)
            }),
        ).await
        .map_err(|_| BrowserError::ChromeError(anyhow::anyhow!("navigation timed out after 20s")))?
        .map_err(|e| BrowserError::ChromeError(anyhow::anyhow!("{}", e)))??;

        Self::post_nav_setup(&tab)?;

        let css_urls: Vec<String> = tab.evaluate(
            "JSON.stringify([...new Set(
                performance.getEntriesByType('resource')
                    .filter(e => e.initiatorType === 'css' && e.name && e.name.includes('.css'))
                    .map(e => e.name)
            )])",
            false,
        )?
        .value
        .and_then(|v| v.as_str().map(String::from))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

        let webpage = WebPage::from_tab(tab, no_conversions, download_videos, css_urls).await?;

        Ok(webpage)
    }

    pub fn url_to_pdf(&self, url: &str) -> Result<()> {

        let tab = Self::open_tab_impl(url, &self.0)?;
        Self::post_nav_setup(&tab)?;
        let title = tab.get_title()?;
        let filename = format!("{}.pdf", safe_title(&title));
        let output_path = Path::new(&filename);
        let pdf = tab.print_to_pdf(None)?;
        std::fs::write(&output_path, pdf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_complicated_website() {
        let b = Browser::new().unwrap();
        let link = "https://100-beste-plakate.de/plakate/";
        //let link = "https://en.wikipedia.org/wiki/%C3%89cole_cantonale_d%27art_de_Lausanne";
        let tab = b.open_tab(link, false, false).await.unwrap();

        tab.write_to_disk("test/complicated_website", false).await.unwrap();
    }
}
