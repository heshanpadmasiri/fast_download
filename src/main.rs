use indicatif::ProgressBar;
use reqwest;
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};
use futures::{
    StreamExt,
    stream::FuturesUnordered
};

#[derive(Debug)]
struct Args {
    url_file_name: PathBuf,
}

#[derive(Debug)]
struct Image {
    url: String,
    file_name: String,
}

#[derive(Debug)]
enum DownloadResult {
    Success,
    Skipped,
    Error(String),
}

#[tokio::main]
async fn main() {
    let args = parse_args().expect("failed to parse args");
    let images = parse_url_file(&args);
    let n_images = images.len();
    let mut futures = FuturesUnordered::new();

    let pb = ProgressBar::new(n_images.try_into().unwrap());
    for image in images {
        let fut = async move {
            if let DownloadResult::Error(err) = download_image(&image).await {
                println!("error : {} url: {} file_name: {}", err, image.url, image.file_name);
            }
        };
        futures.push(fut);
        if futures.len() > 20 {
            futures.next().await.unwrap();
            pb.inc(1);
        }
    }
    while futures.len() > 0 {
        futures.next().await.unwrap();
        pb.inc(1);
    }
    pb.finish_and_clear();
}

fn parse_args() -> Option<Args> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        return None;
    }
    let url_file_name = PathBuf::from(&args[1]);
    if url_file_name.exists() && url_file_name.is_file() {
        return Some(Args { url_file_name });
    }
    None
}

fn parse_url_file(args: &Args) -> Vec<Image> {
    let file = File::open(&args.url_file_name).expect("failed to open url file");
    let reader = BufReader::new(file);
    let mut images = Vec::new();
    for line in reader.lines() {
        let line = line.expect("faild to read line");
        if line.len() == 0 {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            println!("invalid line: {}", line);
            continue;
        }
        let url = parts[0];
        let file_name = parts[1..].join(" ");
        images.push(Image {
            url: url.to_string(),
            file_name: file_name.to_string(),
        });
    }
    images
}

async fn download_image(image: &Image) -> DownloadResult {
    let path = PathBuf::from(&image.file_name);
    if path.exists() {
        return DownloadResult::Skipped;
    }
    match reqwest::get(&image.url).await {
        Ok(response) => {
            let bytes = response.bytes().await;
            match bytes {
                Ok(bytes) => {
                    if let Some(parent) = path.parent() {
                        if let Err(err) = std::fs::create_dir_all(parent) {
                            return DownloadResult::Error(err.to_string());
                        }
                    }
                    match File::create(path) {
                        Ok(mut file) => {
                            if let Err(err) = std::io::copy(&mut bytes.as_ref(), &mut file) {
                                return DownloadResult::Error(err.to_string());
                            }
                        }
                        Err(err) => return DownloadResult::Error(err.to_string()),
                    }
                }
                Err(err) => return DownloadResult::Error(err.to_string()),
            }
            DownloadResult::Success
        }
        Err(err) => DownloadResult::Error(err.to_string()),
    }
}
