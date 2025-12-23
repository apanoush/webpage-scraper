use webpage_scraper::browser;
use clap::Parser;

/// Converts a webpage to a PDF using a headless browser
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// URL of the website to convert to PDF
    url: String,
}

fn main() {

    let args = Args::parse();

    let browser = browser::Browser::new().expect("Can't initiate browser");

    browser.url_to_pdf(&args.url).expect("Can't convert webpage to PDF");

}
