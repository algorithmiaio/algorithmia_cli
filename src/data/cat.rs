use crate::config::Profile;
use crate::CmdRunner;
use algorithmia::data::{DataFile, HasDataPath};
use algorithmia::Algorithmia;
use docopt::Docopt;

use std::io;
use std::vec::IntoIter;

static USAGE: &'static str = r##"Usage: mia cat <data-file>...

  Concatenate file(s) from the Algorithmia Data API and print on standard output
"##;

#[derive(RustcDecodable, Debug)]
struct Args {
    arg_data_file: Vec<String>,
}

pub struct Cat {
    client: Algorithmia,
}
impl CmdRunner for Cat {
    fn get_usage() -> &'static str {
        USAGE
    }

    fn cmd_main(&self, argv: IntoIter<String>) {
        let args: Args = Docopt::new(USAGE)
            .and_then(|d| d.argv(argv).decode())
            .unwrap_or_else(|e| e.exit());

        for f in args.arg_data_file {
            cat_file(&self.client.file(&f))
        }
    }
}

impl Cat {
    pub fn new(profile: Profile) -> Self {
        Cat {
            client: profile.client(),
        }
    }
}

fn cat_file(data_file: &DataFile) {
    match data_file.get() {
        Ok(mut response) => {
            let mut stdout = io::stdout();

            // Copy downloaded data to stdout
            match io::copy(&mut response, &mut stdout) {
                Ok(_) => (),
                Err(err) => quit_err!("Error copying data: {}", err),
            }
        }
        Err(e) => quit_err!("Error downloading {}: {}", data_file.to_data_uri(), e),
    };
}
