# grok-cli

A Grok CLI written in rust. Uses the [`grok`](https://github.com/daschl/grok) library, and [`serde`](https://github.com/serde-rs/serde) to handle serialisation into other formats.

## Usage guide

### Command line options

```
Parse unstructured data using grok filters.

USAGE:
    grok-cli [FLAGS] [OPTIONS] --pattern <pattern> [--] [input]...

FLAGS:
    -h, --help           Prints help information
        --no-patterns    If this option is provided then the grok parsers will not populate the grok parser with all the
                         default patterns
    -s, --stats          Return stats on printing, number of successfully parsed and failed records
    -V, --version        Prints version information

OPTIONS:
    -o, --output <output>                  
    -f, --output-format <output-format>    Provides options for either "Json" or "Csv" output options, case-insensitive.
                                           Default option is Json [default: OutputFormat::Json]
    -p, --pattern <pattern>                Pattern to match
        --patterns <patterns>              Custom patterns directory, uses defaults is not provided
    -r, --rules <rules>...                 Rules field, points to one or more afrs rules

ARGS:
    <input>...    Input field, stores one or more paths, parsed from a file glob
```

### Example 1
Sample data (`sample_data.dat`):
```
0.0.0.0 GET
0.0.0.1 GET
0.0.q1.0 POST
0.1.0.0 GET
1.0.0.0 DELETE
```
`grok-cli` command:
```
$ cat sample_data.dat | grok-cli --patterns .test_data/patterns/ -p '%{IP:ip} %{TEST:req}' -f csv
"ip", "req"
"0.0.0.0", "GET"
"0.0.0.1", "GET"
No matches against data: "0.0.q1.0 POST"
"0.1.0.0", "GET"
"1.0.0.0", "DELETE"
```
Data is printed as it's parsed, from the output we can see one of the sample is not parsable.

### Example 2
Sample data (`sample_data.dat`):
```
0.0.0.0 GET
0.0.0.1 GET
0.0.q1.0 POST
0.1.0.0 GET
1.0.0.0 DELETE
```
`grok-cli` command:
```
$ cat sample_data.dat | grok-cli --patterns .test_data/patterns/ -p '%{IP:ip} %{TEST:req}' -f json -s
{"ip":"0.0.0.0","req":"GET"}
{"ip":"0.0.0.1","req":"GET"}
No matches against data: "0.0.q1.0 POST"
{"ip":"0.1.0.0","req":"GET"}
{"ip":"1.0.0.0","req":"DELETE"}
{"parsed":4,"failed":1}
```
Data is printed as it's parsed, from the output we can see one of the sample is not parsable. Stats are also printed.
