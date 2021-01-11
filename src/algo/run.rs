use super::{display_response, split_args, InputData, ResponseConfig};
use crate::config::Profile;
use crate::CmdRunner;
use algorithmia::algo::{AlgoOptions, Response};
use algorithmia::Algorithmia;
use docopt::Docopt;
use std::vec::IntoIter;

static USAGE: &'static str = r##"Usage:
  algo run [options] <algorithm>

  <algorithm> syntax: USERNAME/ALGONAME[/VERSION]
  Recommend specifying a version since algorithm costs can change between minor versions.

  Input Data Options:
    There are option variants for specifying the type and source of input data.
    If <file> is '-', then input data will be read from STDIN.

    Auto-Detect Data:
      -d, --data <data>             If the data parses as JSON, assume JSON, else if the data
                                      is valid UTF-8, assume text, else assume binary
      -D, --data-file <file>        Same as --data, but the input data is read from a file

    JSON Data:
      -j, --json <data>             Algorithm input data as JSON (application/json)
      -J, --json-file <file>        Same as --json, but the input data is read from a file

    Text Data:
      -t, --text <data>             Algorithm input data as text (text/plain)
      -T, --text-file <file>        Same as --text, but the input data is read from a file

    Binary Data:
      -b, --binary <data>           Algorithm input data as binary (application/octet-stream)
      -B, --binary-file <file>      Same as --data, but the input data is read from a file


  Output Options:
    By default, only the algorithm result is printed to STDOUT while additional notices may be
    printed to STDERR.

    --debug                         Print algorithm's STDOUT (default for 'algo runlocal')
    --no-debug                      Don't print algorithm's STDOUT (default for 'algo run')
    --response-body                 Print HTTP response body (replaces result)
    --response                      Print full HTTP response including headers (replaces result)
    -s, --silence                   Suppress any output not explicitly requested (except result)
    -o, --output <file>             Print result to a file

  Other Options:
    --timeout <seconds>             Sets algorithm timeout

  Examples:
    algo run kenny/factor/0.1.0 -d '79'                   Run algorithm with specified data input
    algo run anowell/Dijkstra -D routes.json              Run algorithm with file input
    algo run anowell/Dijkstra -D - < routes.json          Same as above but using STDIN
    algo run opencv/SmartThumbnail -D in.png -o out.png   Run algorithm saving output to a file
"##;

#[derive(RustcDecodable, Debug)]
struct Args {
    cmd_run: bool,
    arg_algorithm: String,
    flag_response_body: bool,
    flag_response: bool,
    flag_silence: bool,
    flag_debug: bool,
    flag_no_debug: bool,
    flag_output: Option<String>,
    flag_timeout: Option<u32>,
}

pub struct Run {
    client: Algorithmia,
}
impl CmdRunner for Run {
    fn get_usage() -> &'static str {
        USAGE
    }

    fn cmd_main(&self, argv: IntoIter<String>) {
        // We need to preprocess input args before giving other args to Docopt
        let (mut input_args, other_args) = split_args(argv, USAGE);

        // Parse the remaining args with Docopt
        let args: Args = Docopt::new(USAGE)
            .and_then(|d| d.argv(other_args).decode())
            .unwrap_or_else(|e| e.exit());

        // --debug can override --silence, but the lack of --debug respects --silence
        let debug = args.flag_debug || !(args.flag_no_debug || args.flag_silence);

        let mut opts = AlgoOptions::default();
        if debug {
            opts.stdout(true);
        }
        if let Some(timeout) = args.flag_timeout {
            opts.timeout(timeout);
        }

        // Run the algorithm
        let response = self.run_algorithm(&*args.arg_algorithm, input_args.remove(0), opts);

        let config = ResponseConfig {
            flag_response_body: args.flag_response_body,
            flag_response: args.flag_response,
            flag_silence: args.flag_silence,
            flag_debug: debug,
            flag_output: args.flag_output,
        };

        display_response(response, config);
    }
}

impl Run {
    pub fn new(profile: Profile) -> Self {
        Run {
            client: profile.client(),
        }
    }

    fn run_algorithm(&self, algo: &str, input_data: InputData, opts: AlgoOptions) -> Response {
        let mut algorithm = self.client.algo(algo);
        let algorithm = algorithm.set_options(opts);

        let result = match input_data {
            InputData::Text(text) => algorithm.pipe_as(text, mime::TEXT_PLAIN),
            InputData::Json(json) => algorithm.pipe_as(json, mime::APPLICATION_JSON),
            InputData::Binary(bytes) => algorithm.pipe_as(bytes, mime::APPLICATION_OCTET_STREAM),
        };

        match result {
            Ok(response) => response,
            Err(err) => quit_err!("Error calling algorithm: {} {}", 1, err),
        }
    }
}
