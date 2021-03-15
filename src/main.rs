//
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
//

mod glob {
    //! Functions to handle reading data from file.
    use super::Result;
    use glob::glob;
    use std::{
        fs,
        io::{prelude::*, BufReader},
    };

    /// FileInput struct is created using a glob string and can be iterated over for strings representing lines in matching files.
    pub struct FileInput {
        // glob_str: String,
        paths: glob::Paths,
        current_reader: BufReader<fs::File>,
    }

    impl FileInput {
        /// Creates a file FileInput struct, will panic if glob fails to be created and print error.
        pub fn new(glob_str: &str) -> Result<Self> {
            let mut paths = glob(glob_str)?;
            let first_path = match paths.next() {
                Some(res) => res?,
                None => return Err(format!("{} did not return any files", glob_str).into()),
            };
            // Create a reader from first glob
            let file = fs::OpenOptions::new().read(true).open(first_path)?;
            let current_reader = BufReader::new(file);
            // Return Self
            Ok(Self {
                // glob_str: glob_str.to_string(),
                paths,
                current_reader,
            })
        }

        pub fn read_line(&mut self) -> Result<Option<String>> {
            // Check self.current_reader for data
            let mut line = String::new();
            if self.current_reader.read_line(&mut line)? == 0 {
                match self.paths.next() {
                    Some(p) => {
                        // Create a BufReader
                        let file = fs::OpenOptions::new().read(true).open(p?)?;
                        let mut reader = BufReader::new(file);
                        // Read line
                        let mut line = String::new();
                        let _ = reader.read_line(&mut line)?;
                        // Store reader
                        self.current_reader = reader;
                    }
                    None => return Ok(None),
                }
            }

            match line.is_empty() {
                false => Ok(Some(line)),
                true => Ok(None),
            }
        }
    }
}

mod grok_parser {
    //! Grok parser
    use super::Result;
    use grok::{Grok, Pattern};
    use std::{
        collections::BTreeMap,
        fs,
        io::{prelude::*, BufReader},
        path::PathBuf,
    };

    /// GrokParser struct is used to read a Matches from a String.
    #[derive(Debug)]
    pub struct GrokParser {
        grok: Grok,
        pattern: Pattern,
    }

    impl GrokParser {
        pub fn new(pattern: &str, patterns: Option<&PathBuf>, no_patterns: bool) -> Result<Self> {
            let mut grok = match patterns {
                Some(d) => {
                    //
                    let mut g = match no_patterns {
                        true => Grok::empty(),
                        false => Grok::with_patterns(),
                    };
                    for (k, v) in read_aliases(d)? {
                        g.insert_definition(k, v);
                    }
                    g
                }
                None => Grok::with_patterns(),
            };
            //
            let pattern = grok.compile(pattern, true)?;
            //
            Ok(Self { grok, pattern })
        }

        pub fn parse(&self, data: &str) -> Result<BTreeMap<String, String>> {
            match self.pattern.match_against(data) {
                None => Err(format!("No matches against data: {}", data.trim_end()).into()),
                Some(matches) => {
                    let mut map = BTreeMap::new();
                    for (k, v) in matches.iter() {
                        map.insert(k.to_string(), v.to_string());
                    }
                    Ok(map)
                }
            }
        }
    }

    fn read_aliases(patterns: &PathBuf) -> Result<BTreeMap<String, String>> {
        let mut aliases = BTreeMap::new();
        if !patterns.is_dir() && patterns.exists() {
            return Err(
                format!("{} patterns directory does not exist.", patterns.display()).into(),
            );
        }
        //
        for file in fs::read_dir(patterns)? {
            let file = file?;
            // Read lines in file and add them as patterns

            let meta = file.metadata().unwrap();
            if meta.is_file() {
                let reader = BufReader::new(fs::File::open(file.path()).unwrap());
                for line in reader.lines() {
                    if let Ok(l) = line {
                        let (key, value) = l.split_at(l.find(' ').unwrap());
                        aliases.insert(key.to_string(), value.trim_start().to_string());
                    }
                }
            }
        }
        Ok(aliases)
    }
}

mod args {
    //! StructOpt argument functions.
    use std::path::PathBuf;
    use structopt::StructOpt;

    #[derive(Debug, StructOpt)]
    #[structopt(name = "grok", about = "Parse structured data using grok filters.")]
    pub struct Options {
        /// Pattern to match on
        #[structopt(short, long)]
        pub pattern: String,
        /// Input file glob, can match on multiple files.
        #[structopt(short, long)]
        pub input: String,
        /// Output file, stdout if not provided
        #[structopt(short, long, parse(from_os_str))]
        pub output: Option<PathBuf>,
        /// Custom patterns directory, uses defaults is not provided.
        #[structopt(long, parse(from_os_str))]
        pub patterns: Option<PathBuf>,
        /// If this option is provided then the grok parsers will not populate the grok parser with all the default patterns.
        #[structopt(long)]
        pub no_patterns: bool,
        /// Print CSV formatted data. Support for optional fields currently not yet implemented.
        #[structopt(short, long)]
        pub csv: bool,
        /// Print JSON formatted data
        #[structopt(short, long)]
        pub json: bool,
    }
}

fn main() {
    use structopt::StructOpt;
    let opt = args::Options::from_args();
    // Check json and csv options are not both being used.
    if opt.json && opt.csv {
        eprintln!("Select either JSON or CSV but not both options to output");
        std::process::exit(1);
    }
    // Get a file input handle.
    let mut file_in = match glob::FileInput::new(&opt.input) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("FileInput: {}", e);
            std::process::exit(1);
        }
    };
    // Get Grok parser parser. Handle based on options.
    let grok_parser =
        match grok_parser::GrokParser::new(&opt.pattern, opt.patterns.as_ref(), opt.no_patterns) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("GrokParser: {}", e);
                std::process::exit(1);
            }
        };
    //
    let mut headers = Vec::new();
    while let Ok(Some(a)) = file_in.read_line() {
        let parsed = match grok_parser.parse(&a) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("GrokParser: {}", e);
                continue;
            }
        };
        // Print as CSV if option is true
        if opt.csv {
            if headers.is_empty() {
                headers = parsed.keys().map(|k| format!("\"{}\"", k)).collect();
                headers.sort();
                println!("{}", headers.join(", "));
            }
            // Parse in order of vec, otherwise the resulting values are unordered.
            let values: Vec<String> = parsed.values().map(|v| format!("\"{}\"", v)).collect();
            println!("{}", values.join(", "));
        }
        // Print as JSON if option is true
        if opt.json {
            let json = match serde_json::to_string(&parsed) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{}", e);
                    continue;
                }
            };
            println!("{}", json);
        }
    }
}
