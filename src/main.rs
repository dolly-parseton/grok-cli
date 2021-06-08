//
#[macro_use]
extern crate serde;
//
use grok::{Grok, Pattern};
use std::{
    collections::BTreeMap,
    convert::TryFrom,
    error, fs,
    io::{self, prelude::*, BufReader},
    path,
};
use structopt::StructOpt;
//
type Result<T> = std::result::Result<T, Box<dyn error::Error>>;
//
pub enum Input {
    Stdin(io::Stdin),
    Paths {
        paths: Vec<path::PathBuf>,
        buffer: io::BufReader<fs::File>,
    },
}
//
impl TryFrom<Vec<path::PathBuf>> for Input {
    //
    type Error = Box<dyn std::error::Error>;
    //
    fn try_from(mut paths: Vec<path::PathBuf>) -> Result<Self> {
        match paths.pop() {
            None => Ok(Self::Stdin(io::stdin())),
            Some(p) => Ok(Self::Paths {
                paths,
                buffer: io::BufReader::new(fs::OpenOptions::new().read(true).open(p)?),
            }),
        }
    }
}
//
impl Iterator for Input {
    type Item = Result<String>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Stdin(stdin) => stdin.lock().lines().next().map(|r| r.map_err(|e| e.into())),
            Self::Paths {
                ref mut paths,
                ref mut buffer,
            } => {
                // Try read from buffer
                let mut line = String::new();
                match buffer.read_line(&mut line) {
                    Err(_) | Ok(0) => {
                        match paths.pop() {
                            Some(p) => {
                                // Create a BufReader
                                match fs::OpenOptions::new().read(true).open(p) {
                                    Ok(f) => *buffer = io::BufReader::new(f),
                                    Err(e) => return Some(Err(e.into())),
                                }
                                self.next()
                            }
                            None => None,
                        }
                    }
                    Ok(_) => Some(Ok(line)),
                }
            }
        }
    }
}
//
pub enum Output {
    Print,
    File {
        path: path::PathBuf,
        err_path: path::PathBuf,
    },
}
//
impl TryFrom<Option<path::PathBuf>> for Output {
    type Error = Box<dyn error::Error>;
    fn try_from(path: Option<path::PathBuf>) -> Result<Self> {
        match path {
            None => Ok(Self::Print),
            Some(p) => {
                let err_path = p.join(".err");
                match (err_path.exists(), !p.exists()) {
                    (true, true) => Err(format!(
                        "Could not log to {} or {}. Both files already exist.",
                        err_path.display(),
                        p.display()
                    )
                    .into()),
                    (false, true) => {
                        Err(format!("Could not log to {}, file already exists", p.display()).into())
                    }
                    (true, false) => Err(format!(
                        "Could not log to {}, file already exists",
                        err_path.display()
                    )
                    .into()),
                    (false, false) => Ok(Self::File { err_path, path: p }),
                }
            }
        }
    }
}
//
impl Output {
    pub fn output(&self, parse_res: Result<String>) -> Result<()> {
        match self {
            Self::Print => {
                match parse_res {
                    Ok(o) => println!("{}", o),
                    Err(e) => eprintln!("{}", e),
                };
                Ok(())
            }
            Self::File {
                ref path,
                ref err_path,
            } => match parse_res {
                Ok(o) => {
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    let _ = file.write(o.as_bytes())?;
                    file.write("\n".as_bytes())
                }
                Err(e) => {
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(err_path)?;
                    file.write(format!("{}\n", e.to_string()).as_bytes())
                }
            }
            .map(|_| ())
            .map_err(|e| e.into()),
        }
    }
}
//
#[derive(Debug)]
pub struct GrokParser {
    grok: Grok,
    pattern: Pattern,
}

impl GrokParser {
    pub fn new(pattern: &str, patterns: Option<&path::PathBuf>, no_patterns: bool) -> Result<Self> {
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

    pub fn parse(&self, data: &str, stats: &mut Stats) -> Result<BTreeMap<String, String>> {
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

fn read_aliases(patterns: &path::Path) -> Result<BTreeMap<String, String>> {
    let mut aliases = BTreeMap::new();
    if !patterns.is_dir() && patterns.exists() {
        return Err(format!("{} patterns directory does not exist.", patterns.display()).into());
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

#[derive(Default, Serialize)]
pub struct Stats {
    pub parsed: u64,
    pub failed: u64,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "grok", about = "Parse unstructured data using grok filters.")]
pub struct Opt {
    /// Pattern to match
    #[structopt(short, long)]
    pub pattern: String,
    // File to send output to.
    #[structopt(short, long, parse(from_os_str))]
    pub output: Option<path::PathBuf>,
    /// Custom patterns directory, uses defaults is not provided.
    #[structopt(long, parse(from_os_str))]
    pub patterns: Option<path::PathBuf>,
    /// If this option is provided then the grok parsers will not populate the grok parser with all the default patterns.
    #[structopt(long)]
    pub no_patterns: bool,
    /// Provides options for either "Json" or "Csv" output options, case-insensitive. Default option is Json
    #[structopt(short = "f", long, parse(try_from_str = parse_format),default_value = "OutputFormat::Json")]
    pub output_format: OutputFormat,
    /// Return stats on printing, number of successfully parsed and failed records.
    #[structopt(short, long)]
    pub stats: bool,
    /// Input field, stores one or more paths, parsed from a file glob.
    #[structopt(parse(from_os_str))]
    pub input: Vec<path::PathBuf>,
    /// Rules field, points to one or more afrs rules.
    #[structopt(short, long, parse(from_os_str))]
    pub rules: Vec<path::PathBuf>,
}

#[derive(Debug)]
pub enum OutputFormat {
    /// Return JSON formatted data, new line delimited.
    Json,
    /// Return CSV formatted data. Support for optional fields currently not yet implemented.
    Csv,
}
impl OutputFormat {
    fn handle_parsed(&self, parsed: BTreeMap<String, String>, output: &Output) -> Result<()> {
        match self {
            Self::Json => match serde_json::to_string(&parsed) {
                Ok(j) => output.output(Ok(j)),
                Err(e) => output.output(Err(Box::new(e))),
            },
            Self::Csv => output.output(Ok(
                parsed
                    .values()
                    .map(|v| format!("\"{}\"", v))
                    .collect::<Vec<String>>()
                    .join(", "), // .to_string()
            )),
        }
    }
    fn handle_stats(&self, stats: &Stats, output: &Output) -> Result<()> {
        match self {
            Self::Json => match serde_json::to_string(&stats) {
                Ok(p) => output.output(Ok(p)),
                Err(e) => output.output(Err(Box::new(e))),
            },
            Self::Csv => {
                output.output(Ok(vec!["parsed", "failed"].join(", ")))?;
                output.output(Ok(
                    vec![stats.parsed.to_string(), stats.failed.to_string()].join(", ")
                ))
            }
        }
        .map(|_| ())
    }
}
fn parse_format(src: &str) -> Result<OutputFormat> {
    match src.to_ascii_lowercase().as_str() {
        "json" => Ok(OutputFormat::Json),
        "csv" => Ok(OutputFormat::Csv),
        _ => Err(format!("Unable to parse {} to OutputFormat", src).into()),
    }
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    // Get a file input handle.
    let mut input = Input::try_from(opt.input)?;
    // Get Grok parser parser. Handle based on options.
    let grok_parser = GrokParser::new(&opt.pattern, opt.patterns.as_ref(), opt.no_patterns)?;
    // Get the output struct.
    let output = Output::try_from(opt.output)?;
    // Generate a stats component.
    let mut stats = Stats::default();
    let mut headers = Vec::new();
    //
    while let Some(Ok(a)) = input.next() {
        let parsed = match grok_parser.parse(&a, &mut stats) {
            Ok(p) => p,
            Err(e) => {
                output.output(Err(e))?;
                continue;
            }
        };
        // Print as CSV if option is true
        if let OutputFormat::Csv = opt.output_format {
            if headers.is_empty() {
                headers = parsed.keys().map(|k| format!("\"{}\"", k)).collect();
                headers.sort();
                output.output(Ok(headers.join(", ").to_string()))?;
            }
        }
        // Handle parsed data based on output_format.
        opt.output_format.handle_parsed(parsed, &output)?;
    }
    //
    if opt.stats {
        opt.output_format.handle_stats(&stats, &output)?;
    }
    //
    Ok(())
}
