use super::size_with_suffix;
use crate::config::Profile;
use crate::CmdRunner;
use algorithmia::data::{DataFile, DataItem, HasDataPath};
use algorithmia::Algorithmia;
use chan;
use docopt::Docopt;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::vec::IntoIter;
use std::{clone, cmp, fs, io, thread};

static USAGE: &'static str = r##"Usage:
  algo cp [options] <source>... <dest>
  algo copy [options] <source>... <dest>

  Copy files to or from the Algorithmia Data API

  An Algorithmia Data URL must be prefixed with data:// in order to avoid potential path ambiguity

  Options:
    -c <CONCURRENCY>    Number of threads for uploading in parallel [Default: 8]

  Examples:
    algo cp file1.jpg file2.jpg data://.my/foo          Upload 2 files to your 'foo' data directory
    algo cp data://.my/foo/file1.jpg .                  Download file1.jpg to the workig directory
"##;

// TODO:
// -r                   Recursive copy if the source is a directory

#[derive(RustcDecodable, Debug)]
struct Args {
    arg_source: Vec<String>,
    arg_dest: String,
    flag_c: u32,
}

pub struct Cp {
    client: Algorithmia,
}
impl CmdRunner for Cp {
    fn get_usage() -> &'static str {
        USAGE
    }

    fn cmd_main(&self, argv: IntoIter<String>) {
        let args: Args = Docopt::new(USAGE)
            .and_then(|d| d.argv(argv).decode())
            .unwrap_or_else(|e| e.exit());

        let cp_client = CpClient::new(self.client.clone(), args.flag_c, &args.arg_dest);

        // Download if the dest is a local path or prefixed with file://_
        //   otherwise, assume upload
        let dest_parts: Vec<_> = args.arg_dest.splitn(2, "://").collect();
        if dest_parts.len() < 2 || dest_parts[0] == "file" {
            cp_client.download(args.arg_source);
        } else {
            cp_client.upload(args.arg_source);
        }
    }
}

impl Cp {
    pub fn new(profile: Profile) -> Self {
        Cp {
            client: profile.client(),
        }
    }
}

struct CpClient {
    client: Algorithmia,
    max_concurrency: u32,
    dest: Arc<String>,
}

impl clone::Clone for CpClient {
    fn clone(&self) -> CpClient {
        CpClient {
            client: self.client.clone(),
            max_concurrency: self.max_concurrency,
            dest: self.dest.clone(),
        }
    }
}

impl CpClient {
    fn new(client: Algorithmia, max_concurrency: u32, dest: &str) -> CpClient {
        CpClient {
            client: client,
            max_concurrency: max_concurrency,
            dest: Arc::new(dest.to_string()),
        }
    }

    fn upload(&self, sources: Vec<String>) {
        // As long as we aren't recursing, we can be more aggressive in limiting threads we spin up
        // TODO: when supporting dir recursion, fall-back to max_concurrency
        let concurrency = cmp::min(sources.len(), self.max_concurrency as usize);

        let (tx, rx) = chan::sync(self.max_concurrency as usize);
        let wg = chan::WaitGroup::new();
        let completed = Arc::new(Mutex::new(0));

        // One Producer thread queuing up file paths to upload
        thread::spawn(move || {
            for path in sources {
                // TODO: if recursing and is_dir: recurse_and_send(&tx, path)
                tx.send(path);
            }
            drop(tx);
        });

        // Spin up threads to concurrently upload files per that paths received on rx channel
        for _ in 0..concurrency {
            wg.add(1);

            let thread_wg = wg.clone();
            let thread_rx = rx.clone();
            let thread_conn = self.clone();
            let thread_completed = completed.clone();

            thread::spawn(move || {
                for rx_path in thread_rx {
                    let dest_obj = thread_conn.client.data(&*thread_conn.dest);
                    let put_res = match dest_obj.into_type() {
                        // If dest exists as DataFile, overwrite it
                        Ok(DataItem::File(f)) => {
                            let file = File::open(&*rx_path).unwrap();
                            f.put(file).map(|_| f.to_data_uri())
                        }
                        // If dest exists as DataDir, add file to dir
                        Ok(DataItem::Dir(d)) => d
                            .put_file(&rx_path)
                            .map(|_| d.child::<DataFile>(&rx_path).to_data_uri()),
                        // Otherwise, try adding new file with exact path as dest
                        Err(_) => {
                            let file = File::open(&*rx_path).unwrap();
                            let f = thread_conn.client.file(&*thread_conn.dest);
                            f.put(file).map(|_| f.to_data_uri())
                        }
                    };

                    match put_res {
                        Ok(uri) => {
                            println!("Uploaded {}", uri);
                            let mut count = thread_completed.lock().unwrap();
                            *count += 1;
                        }
                        Err(e) => quit_err!("Error uploading {}: {}", rx_path, e),
                    };
                }
                thread_wg.done();
            });
        }

        wg.wait();
        println!("Finished uploading {} file(s)", *completed.lock().unwrap());
    }

    fn download(&self, sources: Vec<String>) {
        // As long as we aren't recursing, we can be more aggressive in limiting threads we spin up
        // TODO: when supporting datadir recursion, fall-back to max_concurrency
        let concurrency = cmp::min(sources.len(), self.max_concurrency as usize);

        let (tx, rx) = chan::sync(self.max_concurrency as usize);
        let wg = chan::WaitGroup::new();
        let completed = Arc::new(Mutex::new(0));

        // One Producer thread queuing up file paths to upload
        thread::spawn(move || {
            for path in sources {
                // TODO: if recursing and is_dir: recurse_remote_and_send(&tx, path)
                tx.send(path);
            }
            drop(tx);
        });

        // Spin up threads to concurrently download files per that paths received on rx channel
        for _ in 0..concurrency {
            wg.add(1);

            let thread_wg = wg.clone();
            let thread_rx = rx.clone();
            let thread_conn = self.clone();
            let thread_completed = completed.clone();

            thread::spawn(move || {
                for rx_path in thread_rx {
                    let my_file = thread_conn.client.file(&*rx_path);
                    match download_file(&my_file, &*thread_conn.dest) {
                        Ok(bytes) => {
                            println!("Downloaded {} ({}B)", rx_path, size_with_suffix(bytes));
                            let mut count = thread_completed.lock().unwrap();
                            *count += 1;
                        }
                        Err(err_msg) => quit_msg!("Failed to download {}: {}", rx_path, err_msg),
                    }
                }
                thread_wg.done();
            });
        }

        wg.wait();
        println!(
            "Finished downloading {} file(s)",
            *completed.lock().unwrap()
        );
    }
}

fn download_file(data_file: &DataFile, local_path: &str) -> Result<u64, String> {
    match data_file.get() {
        Ok(mut response) => {
            let full_path = match fs::metadata(local_path) {
                Ok(ref m) if m.is_dir() => {
                    Path::new(local_path).join(data_file.basename().unwrap())
                }
                _ => Path::new(local_path).to_owned(),
            };

            let mut output = match File::create(full_path) {
                Ok(f) => Box::new(f),
                Err(err) => return Err(format!("Error creating file: {}", err)),
            };

            // Copy downloaded data to the output writer
            match io::copy(&mut response, &mut output) {
                Ok(bytes) => Ok(bytes),
                Err(err) => Err(format!("Error copying data: {}", err)),
            }
        }
        Err(e) => Err(format!(
            "Error downloading ({}): {}",
            data_file.to_data_uri(),
            e
        )),
    }
}
