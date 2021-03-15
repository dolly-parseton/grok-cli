// 
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
// 

mod glob {
    //! Functions to handle reading data from file.
    use glob::glob;
    use super::*;
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
            let mut paths = match glob(glob_str)?;
            let first_path = match paths.next() {
                Some(res) => match res {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(1);
                    }
                },
                None => {
                    eprintln!("{} did not return any files", glob_str);
                    std::process::exit(1);
                }
            };
            // Create a reader from first glob
            let file = match fs::OpenOptions::new().read(true).open(first_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            };
            let current_reader = BufReader::new(file);
            Self {
                // glob_str: glob_str.to_string(),
                paths,
                current_reader,
            }
        }
    }

    impl Iterator for FileInput {
        type Item = String;

        fn next(&mut self) -> Option<Self::Item> {
            // Check self.current_reader for data
            let mut line = String::new();
            match self.current_reader.read_line(&mut line) {
                Ok(_) => (),
                Err(_e) => {
                    match self.paths.next() {
                        Some(p) => {
                            // Create a BufReader
                            let file = match fs::OpenOptions::new().read(true).open(match p {
                                Ok(p) => p,
                                Err(e) => {
                                    eprintln!("{}", e);
                                    std::process::exit(1);
                                }
                            }) {
                                Ok(p) => p,
                                Err(e) => {
                                    eprintln!("{}", e);
                                    std::process::exit(1);
                                }
                            };
                            let mut reader = BufReader::new(file);
                            // Read line
                            let mut line = String::new();
                            let _ = match reader.read_line(&mut line) {
                                Ok(p) => p,
                                Err(e) => {
                                    eprintln!("{}", e);
                                    std::process::exit(1);
                                }
                            };
                            // Store reader
                            self.current_reader = reader;
                        }
                        None => return None,
                    }
                }
            };
            match line.is_empty() {
                false => Some(line),
                true => None,
            }
        }
    }
}

mod grok_parser {
    //! Grok parser
    use grok::{Grok, Pattern};
    use std::{
        collections::HashMap,
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
        pub fn new(pattern: &str, patterns: Option<&PathBuf>) -> Self {
            let mut grok = match patterns {
                Some(d) => {
                    //
                    let mut g = Grok::empty();
                    for (k, v) in read_aliases(d) {
                        g.insert_definition(k, v);
                    }
                    g
                }
                None => Grok::with_patterns(),
            };
            //
            let pattern = match grok.compile(pattern, true) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            };
            //
            Self { grok, pattern }
        }

        pub fn parse(&self, data: &str) -> HashMap<String, String> {
            match self.pattern.match_against(data) {
                None => HashMap::new(),
                Some(matches) => {
                    let mut map = HashMap::new();
                    for (k, v) in matches.iter() {
                        map.insert(k.to_string(), v.to_string());
                    }
                    map
                }
            }
        }
    }

    fn read_aliases(patterns: &PathBuf) -> Result<HashMap<String, String> {
        let mut aliases = HashMap::new();
        if !patterns.is_dir() && patterns.exists() {
            eprintln!("{} patterns directory does not exist.", patterns.display());
            std::process::exit(1);
        }
        //
        for file in match fs::read_dir(patterns) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        } {
            // Read lines in file and add them as patterns
            if let Ok(f) = file {
                let meta = f.metadata().unwrap();
                if meta.is_file() {
                    let reader = BufReader::new(fs::File::open(f.path()).unwrap());
                    for line in reader.lines() {
                        if let Ok(l) = line {
                            let (key, value) = l.split_at(l.find(" ").unwrap());
                            aliases.insert(key.to_string(), value.to_string());
                        }
                    }
                }
            }
        }
        aliases
    }
}

mod args {
    //! StructOpt argument functions.
    use std::path::PathBuf;
    use structopt::StructOpt;

    #[derive(Debug, StructOpt)]
    #[structopt(name = "example", about = "An example of StructOpt usage.")]
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
        pub patterns_dir: Option<PathBuf>,
    }
}

fn main() {
    use structopt::StructOpt;
    let opt = args::Options::from_args();
    println!("{:?}", opt);

    let mut file_in = glob::FileInput::new(&opt.input);
    let grok_parser = grok_parser::GrokParser::new(&opt.pattern, opt.patterns_dir.as_ref());

    while let Some(a) = file_in.next() {
        println!("{:?}", grok_parser.parse(&a));
        // std::process::exit(1);
    }
}
