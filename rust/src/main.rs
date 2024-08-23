use std::fs::{self,File, OpenOptions};
use std::io::{self,BufReader, BufWriter, Read,BufRead, Write, };
use regex::Regex;
use clap::{App, Arg};
use std::env;
use std::process::Command;
use std::convert::TryInto;
use reqwest::header::{HeaderMap,HeaderValue,HeaderName};
use reqwest::blocking::{Client};
use reqwest::Url;
use std::error::Error as OtherError;
use percent_encoding::{percent_decode_str};

use std::sync::mpsc::{channel, Sender};

use std::thread;
use std::time::{Instant,Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
#[derive(Clone)]
struct DownloadInfo {
    url: String,
    headers: HeaderMap,
}

impl DownloadInfo {
    fn new(url: String, headers: HeaderMap) -> Self {
        DownloadInfo { url: url, headers: headers }
    }
}


fn download_ef2(ef2_filename: &str, parallel: usize) {
    let infos = match parse_download_info(ef2_filename) {
        Ok(infos) => infos,
        Err(e) => {
            println!("File parsing error: {}", e);
            return;
        }
    };
    let infos_len = infos.len();
    let handles: Vec<_> = infos.into_iter().enumerate().map(|(_i, info)| {
        thread::spawn(move || {
            if let Err(e) = download_resource(&info, parallel, infos_len) {
                println!("Download resource error: {}", e);
            }
        })
    }).collect();
    for handle in handles {
        handle.join().unwrap();
    }
}

fn parse_download_info_single(lines_buffer: &[String], re_referer: &Regex, re_user_agent: &Regex) -> Result<Option<DownloadInfo>, Box<dyn std::error::Error>> {
    let url = lines_buffer[0].to_string();
    let referer_match = re_referer.captures(&lines_buffer[1])
        .and_then(|caps| caps.get(1))
        .ok_or("无法获取Referer")?;
    let referer = referer_match.as_str().trim().to_string();

    let user_agent_match = re_user_agent.captures(&lines_buffer[2])
        .and_then(|caps| caps.get(1))
        .ok_or("无法获取User-Agent")?;
    let user_agent = user_agent_match.as_str().trim().to_string();

    let referer_header_name = HeaderName::from_static("referer");
    let user_agent_header_name = HeaderName::from_static("user-agent");

    let referer_header_value = HeaderValue::from_str(&referer)?;
    let user_agent_header_value = HeaderValue::from_str(&user_agent)?;

    let mut header_map = HeaderMap::new();
    header_map.insert(referer_header_name, referer_header_value);
    header_map.insert(user_agent_header_name, user_agent_header_value);

    Ok(Some(DownloadInfo::new(url, header_map)))
}


fn parse_download_info(file_path: &str) -> Result<Vec<DownloadInfo>, Box<dyn std::error::Error>> {
    let lower_file_path = file_path.to_lowercase();
    if lower_file_path.starts_with("http://") || lower_file_path.starts_with("https://") {
        let mut header_map = HeaderMap::new();
        let value = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/53.0.2763.0 Safari/537.36";
        let header_value = HeaderValue::from_str(value)?;

        header_map.insert("User-Agent", header_value);

        return Ok(vec![DownloadInfo::new(file_path.to_string(), header_map)]);
    }

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut infos = Vec::new();
    let mut lines_buffer = Vec::new();
    let mut collecting = false;

    let re_referer = Regex::new(r"referer: (.*)")?;
    let re_user_agent = Regex::new(r"User-Agent: (.*)")?;
    for line_result in reader.lines() {
        let line = line_result?;
        let line = line.trim();
        if line.starts_with('<') {
            collecting = true;
            lines_buffer.clear();
        } else if line.starts_with('>') {
            collecting = false;
            if lines_buffer.len() >= 3 {
                match parse_download_info_single(&lines_buffer, &re_referer, &re_user_agent) {
                    Ok(Some(download_info)) => infos.push(download_info),
                    Ok(None) => (),
                    Err(e) => eprintln!("Error parsing download info: {}", e),
                }
            }

            lines_buffer.clear();
        } else if collecting {
            lines_buffer.push(line.to_string());
        }
    }

    Ok(infos)
}

fn main() {
    let matches = App::new("ef2_downloader")
        .version("1.0")
        .author("Your Name <your.email@example.com>")
        .about("Downloads EF2 files with optional screen support")
        .arg(
            Arg::with_name("noscreen")
                .short('s')
                .long("noscreen")
                .help("Disable screen mode")
                .takes_value(false)
        )
        .arg(
            Arg::with_name("parallel")
                .short('p')
                .long("parallel")
                .help("Number of parallel downloads")
                .default_value("10")
        )
        .arg(
            Arg::with_name("ef2_files")
                .help("EF2 files to download")
                .required(true)
                .multiple(true)
                .allow_invalid_utf8(true)
        )
        .get_matches();

    let mut no_screen = matches.is_present("noscreen");
    let parallel = matches.value_of("parallel").unwrap().parse::<u32>().unwrap();
    let ef2_files: Vec<String> = matches
        .values_of_os("ef2_files")
        .unwrap()
        .map(|os_str| os_str.to_string_lossy().into_owned())
        .collect();

    if ef2_files.is_empty() {
        eprintln!("Must specify EF2 file names");
        std::process::exit(1);
    }
    if env::consts::OS == "windows"{
        no_screen = true;
    }

    for ef2_file in ef2_files {
        let pwd = env::current_dir().unwrap().display().to_string();
        let ef2_filename = ef2_file;
        if no_screen {
            println!("Current directory: {:?}, download file: {:?}", pwd, ef2_filename);
            download_ef2(&ef2_filename, parallel.try_into().unwrap());
        } else {
            println!("Current directory: {:?}, download file: {} has been executed in background screen, you can use 'screen -r' to check the download progress.", pwd, ef2_filename);
            let args: Vec<String> = env::args().collect();
            let command = format!("{} {} --noscreen -p {}", args[0], ef2_filename, parallel);
            match Command::new("screen")
                .args(&["-dmS", "my_screen", "bash", "-c", &command])
                .output() {
                Ok(output) => {
                    println!("Standard output: {}", String::from_utf8_lossy(&output.stdout));
                    println!("Standard error: {}", String::from_utf8_lossy(&output.stderr));
                },
                Err(e) => {
                    println!("An error occurred: {}", e);
                },
            }
        }
    }
}

fn download_resource(info: &DownloadInfo,mut parallel: usize, all_task_count: usize) -> Result<(),Box<dyn std::error::Error>> {
    let (filename, filesize, crc64) = download_header(info)?;
    let filesize = filesize as u64;
    let file_info = fs::metadata(&filename).ok();
    if let Some(fi) = file_info.as_ref() {
        if fi.len() == filesize {
            println!("文件已经存在: {}", filename);
            return Ok(());
        } else if fi.len() != filesize {
            println!("文件已经存在: {}, 但是文件大小不对，重新下载", filename);
        }
    }

    let pwd = env::current_dir().unwrap();
    let temp_dir = pwd.join(format!("{}downloading", filename));
    fs::create_dir_all(&temp_dir)?;

    let filesize = filesize as usize;
    if filesize > 10 * 1024 * 1024 * 1024 && parallel == 10 {
        parallel = (filesize / (1024 * 1024 * 1024)) + 1;
    }
    if filesize < 1024 * 1024 && parallel == 10 {
        parallel = (filesize / (100 * 1024)) + 1;
    }

    let part_size = ((filesize - 1) / parallel) + 1;

    let (tx, rx) = channel();
    let mut part_filenames = vec![String::new(); parallel as usize];
    let mut handles = vec![];
    for i in 0..parallel {
        let range_begin = i * part_size;
        let range_end = if i + 1 == parallel {
            filesize - 1
        } else {
            (i + 1) * part_size - 1
        };
        let full_filename = temp_dir.join(format!("{}.{}-{}", filename, range_begin, range_end)).to_str().unwrap().to_string();
        part_filenames[i as usize] = full_filename.clone();

        let info = info.clone();
        let tx = tx.clone();
        let handle = thread::spawn(move || {
            match download_part(i, info, range_begin.try_into().unwrap(), range_end.try_into().unwrap(), &full_filename, tx){
                Ok(())=>{},
                Err(e)=>{eprintln!("下载分片{}失败：{}",i,e)}
            }
        });
        handles.push(handle);
    }

    drop(tx);
    let mut ar_tasks = vec![Task { index: 0, percent: 0, speed: 0, errstring: "".to_string() }; parallel];
    let mut tick = Instant::now();
    let mut is_error = false;
    let mut finish = 0;
    while let Ok(task) = rx.recv() {
        ar_tasks[task.index] = task.clone();
        if !task.errstring.is_empty(){
            is_error = true;
        }
        if task.percent >= 100 {
            finish += 1;
        }
        if finish >= parallel as isize {
            println!("finish");
            break;
        }
        if tick.elapsed().as_secs() > all_task_count as u64 * 2 {
            tick = Instant::now();
            let mut percent_sum = 0;
            let mut speed_sum = 0;
            for task in &ar_tasks {
                percent_sum += task.percent;
                speed_sum += task.speed;
            }
            let percent_avg = percent_sum as f64 / parallel as f64;
            let left_time = calculate_left_time(speed_sum, filesize.try_into().unwrap(), (percent_avg * filesize as f64 / 100.0) as i64);
            print!("\r{}: {:.2}% {} {}", filename, percent_avg, human_size(speed_sum), left_time);
            io::stdout().flush().unwrap();
        }
    }

    for handle in handles {
        handle.join().unwrap();
    }

    if !is_error {
        let output_file_str = pwd.join(filename).to_str().unwrap().to_string();
        let result = merged_files(part_filenames, output_file_str.to_string(), crc64);
        if result.is_ok() {
            fs::remove_dir_all(temp_dir)?;
        } else {
            println!("合并文件失败，err: {:?}", result.err());
        }
    }

    Ok(())
}

fn download_header(info: &DownloadInfo) -> Result<(String, i64, u64), Box<dyn OtherError>> {
    let mut headers = info.headers.iter().fold(HeaderMap::new(), |mut acc, (k, v)| {
        acc.append(k.clone(), v.clone());
        acc
    });
    headers.insert("Range", HeaderValue::from_static("bytes=0-0"));
    let client = Client::new();
    let response = client
        .get(&info.url)
        .headers(headers)
        .send()?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        println!("{}",info.url);
        return Err(format!("请求文件名文件大小等信息失败，状态码:{},url:{}", response.status(), info.url).into());
    }

    let content_disposition = response.headers().get("content-disposition").and_then(|h| h.to_str().ok());

    let re = Regex::new(r"filename\*=UTF-8''(.+)")?;
    let filename = if let Some(content_disposition) = content_disposition {
        if let Some(caps) = re.captures(content_disposition) {
            if let Some(m) = caps.get(1) {
                let matched = m.as_str();
                percent_decode_str(matched)
                    .decode_utf8_lossy()
                    .into_owned()
            } else {
                "".into() // 返回空字符串的解码结果
            }
        } else {
            let parsed_url = Url::parse(&info.url)?;
            let path_segment = parsed_url.path_segments().and_then(|segments| segments.last());
            percent_decode_str(path_segment.map_or("", |s| s))
                .decode_utf8_lossy()
                .into_owned()
        }
    } else {
        let parsed_url = Url::parse(&info.url)?;
        let path_segment = parsed_url.path_segments().and_then(|segments| segments.last());
        percent_decode_str(path_segment.map_or("", |s| s))
            .decode_utf8_lossy()
            .into_owned()
    };

    let filename = sanitize_filename(filename);

    let content_range = response.headers().get("content-range").and_then(|h| h.to_str().ok());
    let total = if let Some(content_range) = content_range {
        let parts: Vec<&str> = content_range.split('/').collect();
        if parts.len() != 2 {
            return Err("Content-Range格式错误".into());
        }
        parts[1].parse::<i64>()?
    } else {
        return Err("Content-Range header not found".into());
    };

    let filesize = total;

    let crc64 = response
        .headers()
        .get("x-oss-hash-crc64ecma")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    Ok((filename, filesize, crc64))
}

/// 清洗文件名
fn sanitize_filename(filename: String) -> String {
    let illegal_chars = r#"\/:*?"<>|"#;
    illegal_chars.chars().fold(filename, |acc, c| acc.replace(c, "_"))
}




// 定义 Task 结构
#[derive(Debug, Clone)]
struct Task {
    index: usize,
    errstring: String,
    speed: i64,
    percent: i32,
}

fn download_part(
    index: usize,
    info: DownloadInfo,
    range_begin: i64,
    range_end: i64,
    fullfilename: &str,
    p: Sender<Task>,
) -> Result<(), Box<dyn std::error::Error>> {
    let filesize = match std::fs::metadata(fullfilename) {
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    };
    let filesize = filesize as i64;
    let mut task = Task {
        index,
        errstring: "".to_string(),
        speed: 0,
        percent: 0,
    };

    if filesize > (range_end - range_begin) {
        println!("{} 已经下载完成, filesize: {}, rangesize: {}", index, filesize, range_end - range_begin + 1);
        task.percent = 100;
        p.send(task).unwrap();
        return Ok(());
    } else {
        println!("{} 继续下载, filesize: {}, rangesize: {}", index, filesize, range_end - range_begin + 1);
    }

    let client = Client::new();
    let req = client.get(info.url.trim())
        .header("Range", format!("bytes={}-{}", filesize+range_begin, range_end))
        .headers(info.headers)
        .build()?;

    let mut resp = client.execute(req)?;
    if resp.status().is_success() || resp.status().as_u16() == 206 {
        let content_length = resp.content_length().unwrap() as i64;
        let mut file = OpenOptions::new().create(true).append(true).open(fullfilename)?;
        let mut buffer = vec![0; 10 * 1024];
        let mut writer = BufWriter::new(&mut file);
        let mut reader = BufReader::new(&mut resp);
        let mut written = 0;
        let mut last_receive = 0;
        let mut last_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        loop {
            let nr = reader.read(&mut buffer)?;
            if nr > 0 {
                writer.write_all(&buffer[..nr])?;
                written += nr;
                last_receive += nr;
                let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
                if current_time - last_time > 1 {
                    let new_task = Task {
                        index,
                        errstring: "".to_string(),
                        percent: ((written as i64 + filesize) * 100 / (content_length + filesize)) as i32,
                        speed: last_receive as i64 / (current_time - last_time),
                    };

                    p.send(new_task).unwrap();
                    last_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
                    last_receive = 0;
                }
            }
            if nr == 0 { break; }
        }
        let new_task = Task {
            index,
            errstring: "".to_string(),
            percent : 100,
            speed : 0
        };
        p.send(new_task).unwrap();
    } else {
        return Err(Box::new(io::Error::new(io::ErrorKind::Other, format!("请求分片失败，状态码：{}", resp.status()))));
    }

    Ok(())
}

/// 计算剩余时间
fn calculate_left_time(speed: i64, content_size: i64, downloaded_size: i64) -> String {
    if speed == 0 {
        return "NaN".to_string();
    }

    let remaining_bytes = content_size - downloaded_size;
    let remaining_seconds = (remaining_bytes as f64) / (speed as f64);
    let duration = Duration::from_secs(remaining_seconds as u64);

    let hours = duration.as_secs() / 3600;
    let minutes = (duration.as_secs() % 3600) / 60;
    let seconds = duration.as_secs() % 60;

    let mut time_str = String::new();

    if hours > 0 {
        time_str.push_str(&format!("{}h", hours));
    }
    if minutes > 0 || !time_str.is_empty() {
        time_str.push_str(&format!("{}m", minutes));
    }
    time_str.push_str(&format!("{}s", seconds));

    time_str
}

/// 合并文件
fn merged_files(files: Vec<String>, output_file: String, crc64: u64) -> io::Result<()> {
    let mut out_file = File::create(&output_file)?;
    for filename in &files {
        let mut in_file = File::open(filename)?;
        io::copy(&mut in_file, &mut out_file)?;
    }

    if crc64 != 0 {
        let crc64check = crc64_checksum(&output_file)?;
        if crc64check != crc64 {
            return Err(io::Error::new(io::ErrorKind::Other,
                                      format!("crc64Checksum failed, crc64check: {}, crc64: {}", crc64check, crc64)));
        }
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    #[test]
    fn test_human_size(){
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn another() {
        println!("Make this test fail");
    }
}

/// 格式化文件大小
fn human_size(size_bytes: i64) -> String {
    if size_bytes == 0 {
        return "0B".to_string();
    }

    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let unit: i32 = 1024;
    let i = ((size_bytes as f64).log(unit as f64)).floor() as usize;
    let p = unit.pow(i as u32);
    let s = (size_bytes as f64) / (p as f64);
    format!("{:.2}{}", s.round(), units[i])
}

use crc64fast::Digest;
fn crc64_checksum(file_path: &str) -> Result<u64, io::Error> {
    // 打开文件
    let mut file = File::open(file_path)?;

    // 创建一个 CRC64 哈希计算器，并使用 ECMA 多项式初始化表
    let mut hasher = Digest::new();

    // 创建一个缓冲区用于读取文件内容
    let mut buffer = [0; 4096];
    loop {
        // 从文件中读取数据到缓冲区
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        // 更新 CRC64 哈希值
        hasher.write(&buffer[..bytes_read]);
    }

    // 获取最终的 CRC64 校验和
    Ok(hasher.sum64())
}