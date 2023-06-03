use futures::{stream::FuturesUnordered, StreamExt};
use indicatif::ProgressBar;
use reqwest;
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

#[derive(Debug)]
struct Args {
    url_file_name: PathBuf,
    ignore_download_errors: bool,
    verbose: bool,
    force_redownload: bool,
}

#[derive(Debug)]
struct Image {
    url: String,
    file_name: String,
}

#[derive(Debug)]
enum DownloadCompleted {
    Success,
    Skipped,
}

#[derive(Debug)]
enum DownloadError {
    FailedToCreateParentDirectory,
    FailedToCreateFile,
    FailedToDownloadToFile,
    FailedToConvertResponseToBytes,
    FailedToGetUrl,
}

type DownloadResult = Result<DownloadCompleted, DownloadError>;

#[tokio::main]
async fn main() {
    let args = parse_args().expect("failed to parse args");
    let images = parse_url_file(&args);
    let n_images = images.len();
    let mut futures = FuturesUnordered::new();

    let pb = ProgressBar::new(n_images.try_into().unwrap());
    for image in images {
        let fut = async move {
            match download_image(&image, args.force_redownload).await {
                Err(err) => {
                    println!(
                        "error : {:?} url: {} file_name: {}",
                        err, image.url, image.file_name
                    );
                    if !args.ignore_download_errors {
                        panic!("exiting due to error");
                    }
                }
                Ok(DownloadCompleted::Skipped) => {
                    if args.verbose {
                        println!("skipped: {}", image.file_name);
                    }
                }
                Ok(DownloadCompleted::Success) => {
                    if args.verbose {
                        println!("downloaded: {}", image.file_name);
                    }
                }
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
    if args.len() < 2 {
        return None;
    }
    let first = &args[1];
    match first.as_str() {
        "-h" => {
            println!("usage: {} <url_file_name> [-i] [-v] [-f]", args[0]);
            return None;
        }
        filename => {
            let url_file_name = PathBuf::from(filename);
            if url_file_name.exists() && url_file_name.is_file() {
                let ignore_download_errors = args.contains(&"-i".to_string());
                let verbose = args.contains(&"-v".to_string());
                let force_redownload = args.contains(&"-f".to_string());
                return Some(Args {
                    url_file_name,
                    ignore_download_errors,
                    verbose,
                    force_redownload,
                });
            }
            println!("invalid url file: {}", filename);
            return None;
        }
    }
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

async fn download_image(image: &Image, force_redownload: bool) -> DownloadResult {
    let path = PathBuf::from(&image.file_name);
    if path.exists() {
        if force_redownload {
            if let Err(_) = std::fs::remove_file(&path) {
                return Err(DownloadError::FailedToCreateFile);
            }
        } else {
            return Ok(DownloadCompleted::Skipped);
        }
    }
    match reqwest::get(&image.url).await {
        Ok(response) => {
            let bytes = response.bytes().await;
            match bytes {
                Ok(bytes) => {
                    if let Some(parent) = path.parent() {
                        if let Err(_) = std::fs::create_dir_all(parent) {
                            return Err(DownloadError::FailedToCreateParentDirectory);
                        }
                    }
                    match File::create(path) {
                        Ok(mut file) => {
                            if let Err(_) = std::io::copy(&mut bytes.as_ref(), &mut file) {
                                return Err(DownloadError::FailedToDownloadToFile);
                            }
                        }
                        Err(_) => return Err(DownloadError::FailedToCreateFile),
                    }
                }
                Err(_) => return Err(DownloadError::FailedToConvertResponseToBytes),
            }
            Ok(DownloadCompleted::Success)
        }
        Err(_) => Err(DownloadError::FailedToGetUrl),
    }
}
