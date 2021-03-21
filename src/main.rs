//
#[macro_use]
extern crate serde;
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

mod outputs {
    //! Output components (file and print to start)
    //! Output Trait for ensuring Box<dyn Output> is possible
    use super::Result;
    use std::{fs, path::PathBuf};
    pub trait Output {
        fn output(&self, parse_res: Result<String>) -> Result<()>;
    }
    // Print output
    pub struct PrintOutput;
    impl Output for PrintOutput {
        fn output(&self, parse_res: Result<String>) -> Result<()> {
            match parse_res {
                Ok(o) => println!("{}", o),
                Err(e) => eprintln!("{}", e),
            }
            Ok(())
        }
    }
    // File output
    pub struct FileOutput {
        path: PathBuf,
        err_path: PathBuf,
    }
    impl FileOutput {
        pub fn new(path: PathBuf) -> Result<Self> {
            let err_path = path.join(".err");
            Err(match (err_path.exists(), !path.exists()) {
                (true, true) => format!(
                    "Could not log to {} or {}. Both files already exist.",
                    err_path.display(),
                    path.display()
                ),
                (false, true) => {
                    format!("Could not log to {}, file already exists", path.display())
                }
                (true, false) => format!(
                    "Could not log to {}, file already exists",
                    err_path.display()
                ),
                (false, false) => return Ok(Self { err_path, path }),
            }
            .into())
        }
    }
    impl Output for FileOutput {
        fn output(&self, parse_res: Result<String>) -> Result<()> {
            use std::io::Write;
            match parse_res {
                Ok(o) => {
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&self.path)?;
                    let _ = file.write(o.as_bytes())?;
                    let _ = file.write("\n".as_bytes())?;
                }
                Err(e) => {
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&self.err_path)?;
                    let _ = file.write(format!("{}\n", e.to_string()).as_bytes())?;
                }
            }
            Ok(())
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

        pub fn parse(
            &self,
            data: &str,
            stats: &mut super::stats::Stats,
        ) -> Result<BTreeMap<String, String>> {
            match self.pattern.match_against(data) {
                None => {
                    stats.failed += 1;
                    Err(format!("No matches against data: \"{}\"", data.trim_end()).into())
                }
                Some(matches) => {
                    stats.parsed += 1;
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

mod stats {
    #[derive(Default, Serialize)]
    pub struct Stats {
        pub parsed: u64,
        pub failed: u64,
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
        /// Output file, stdout if not provided.
        #[structopt(short, long, parse(from_os_str))]
        pub output: Option<PathBuf>,
        /// Custom patterns directory, uses defaults is not provided.
        #[structopt(long, parse(from_os_str))]
        pub patterns: Option<PathBuf>,
        /// If this option is provided then the grok parsers will not populate the grok parser with all the default patterns.
        #[structopt(long)]
        pub no_patterns: bool,
        /// Return CSV formatted data. Support for optional fields currently not yet implemented.
        #[structopt(short, long)]
        pub csv: bool,
        /// Return JSON formatted data.
        #[structopt(short, long)]
        pub json: bool,
        /// Return stats on printing, number of successfully parsed and failed records.
        #[structopt(short, long)]
        pub stats: bool,
        // Todo features
        // * Pattern Dictionary (ie. A root pattern and varients)
        // * Matching multiple patterns and testing for a match
        // * Stats on matched and failed lines and logging failures (or eprint if no output file is provided)
    }
}

fn main() -> Result<()> {
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
    // Get the generic output struct.
    let output: Box<dyn outputs::Output> = match opt.output {
        Some(p) => Box::new(match outputs::FileOutput::new(p) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }),
        None => Box::new(outputs::PrintOutput),
    };
    // Generate a stats component.
    let mut stats = stats::Stats::default();
    let mut headers = Vec::new();
    while let Ok(Some(a)) = file_in.read_line() {
        let parsed = match grok_parser.parse(&a, &mut stats) {
            Ok(p) => p,
            Err(e) => {
                output.output(Err(e))?;
                // eprintln!("GrokParser: {}", e);
                continue;
            }
        };
        // Print as CSV if option is true
        if opt.csv {
            if headers.is_empty() {
                headers = parsed.keys().map(|k| format!("\"{}\"", k)).collect();
                headers.sort();
                output.output(Ok(headers.join(", ").to_string()))?;
                // println!("{}", headers.join(", "));
            }
            // Parse in order of vec, otherwise the resulting values are unordered.
            let values: Vec<String> = parsed.values().map(|v| format!("\"{}\"", v)).collect();
            output.output(Ok(values.join(", ").to_string()))?;
            // println!("{}", values.join(", "));
        }
        // Print as JSON if option is true
        if opt.json {
            let json = match serde_json::to_string(&parsed) {
                Ok(p) => p,
                Err(e) => {
                    output.output(Err(Box::new(e)))?;
                    continue;
                }
            };
            output.output(Ok(json))?;
        }
    }
    // If stats flag was selectedf output the stats data
    // Print as CSV if option is true
    if opt.csv {
        output.output(Ok(vec!["parsed", "failed"].join(", ")))?;
        output.output(Ok(
            vec![stats.parsed.to_string(), stats.failed.to_string()].join(", ")
        ))?;
        // println!("{}", values.join(", "));
    }
    // Print as JSON if option is true
    if opt.json {
        match serde_json::to_string(&stats) {
            Ok(p) => output.output(Ok(p))?,
            Err(e) => output.output(Err(Box::new(e)))?,
        }
    }
    Ok(())
}
