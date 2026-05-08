use clap::{Parser, ValueEnum};
use std::{collections::HashSet, mem};

use rubydex::{
    indexing, integrity, listing,
    model::graph::Graph,
    resolution::Resolver,
    stats::{
        memory::MemoryStats,
        timer::{Timer, time_it},
    },
    dot,
};

#[derive(Parser, Debug)]
#[command(name = "rubydex_cli", about = "A Static Analysis Toolkit for Ruby", version)]
#[allow(clippy::struct_excessive_bools)]
struct Args {
    #[arg(value_name = "PATHS", default_value = ".")]
    paths: Vec<String>,

    #[arg(long = "stop-after", help = "Stop after the given stage")]
    stop_after: Option<StopAfter>,

    #[arg(long = "dot", help = "Output a DOT graph visualization")]
    dot: bool,

    #[arg(long = "dot-show-builtins", default_value = "false", num_args = 1, value_parser = clap::builder::BoolishValueParser::new(), help = "Include built-in declarations in DOT output")]
    dot_show_builtins: bool,

    #[arg(long = "dot-show-documents", default_value = "true", num_args = 1, value_parser = clap::builder::BoolishValueParser::new(), help = "Show document nodes in DOT output")]
    dot_show_documents: bool,

    #[arg(long = "dot-show-definitions", value_delimiter = ',', help = "Filter DOT definitions by kind (e.g. Class,Module,Method)")]
    dot_show_definitions: Option<Vec<String>>,

    #[arg(long = "dot-show-declarations", value_delimiter = ',', help = "Filter DOT declarations by kind (e.g. Class,Module,Method)")]
    dot_show_declarations: Option<Vec<String>>,

    #[arg(long = "dot-show-edges", value_delimiter = ',', help = "Filter DOT edges by type (e.g. defines,declares,contains,inherits,includes,prepends,extends,owns)")]
    dot_show_edges: Option<Vec<String>>,

    #[arg(long = "stats", help = "Show detailed performance statistics")]
    stats: bool,

    #[arg(long = "check-integrity", help = "Check the integrity of the graph after resolution")]
    check_integrity: bool,

    #[arg(
        long = "report-orphans",
        value_name = "PATH",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "/tmp/rubydex-orphan-report.txt",
        help = "Write orphan definitions report to specified file"
    )]
    report_orphans: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum StopAfter {
    Listing,
    Indexing,
    Resolution,
}

fn exit(print_stats: bool) {
    if print_stats {
        Timer::print_breakdown();
        MemoryStats::print_memory_usage();
    }

    std::process::exit(0);
}

fn main() {
    let args = Args::parse();

    if !args.dot
        && (args.dot_show_builtins
            || !args.dot_show_documents
            || args.dot_show_definitions.is_some()
            || args.dot_show_declarations.is_some()
            || args.dot_show_edges.is_some())
    {
        eprintln!("Error: --dot-* options require --dot");
        std::process::exit(1);
    }

    if args.stats {
        Timer::set_global_timer(Timer::new());
    }

    // Listing

    let (file_paths, errors) = time_it!(listing, { listing::collect_file_paths(args.paths, &HashSet::new()) });

    for error in errors {
        eprintln!("{error}");
    }

    if let Some(StopAfter::Listing) = args.stop_after {
        return exit(args.stats);
    }

    // Indexing

    let mut graph = Graph::new();
    let errors = time_it!(indexing, { indexing::index_files(&mut graph, file_paths) });

    for error in errors {
        eprintln!("{error}");
    }

    if let Some(StopAfter::Indexing) = args.stop_after {
        return exit(args.stats);
    }

    // Resolution

    time_it!(resolution, {
        let mut resolver = Resolver::new(&mut graph);
        resolver.resolve();
    });

    if let Some(StopAfter::Resolution) = args.stop_after {
        return exit(args.stats);
    }

    // Integrity check
    if args.check_integrity {
        let errors = time_it!(integrity_check, { integrity::check_integrity(&graph) });

        if errors.is_empty() {
            println!("Integrity check passed: no issues found");
        } else {
            eprintln!("Integrity check found {} issue(s):", errors.len());

            for error in &errors {
                eprintln!("  - {error}");
            }

            std::process::exit(1);
        }
    }

    // Querying

    if args.stats {
        time_it!(querying, {
            graph.print_query_statistics();
        });
    }

    if args.stats {
        Timer::print_breakdown();
        MemoryStats::print_memory_usage();
    }

    // Orphan report
    if let Some(ref path) = args.report_orphans {
        match std::fs::File::create(path) {
            Ok(mut file) => {
                if let Err(e) = graph.write_orphan_report(&mut file) {
                    eprintln!("Failed to write orphan report: {e}");
                } else {
                    println!("Orphan report written to {path}");
                }
            }
            Err(e) => eprintln!("Failed to create orphan report file: {e}"),
        }
    }

    // Generate visualization or print statistics
    if args.dot {
        println!("{}", dot::DotBuilder::generate(
            &graph,
            args.dot_show_builtins,
            args.dot_show_documents,
            args.dot_show_declarations.as_deref(),
            args.dot_show_definitions.as_deref(),
            args.dot_show_edges.as_deref(),
        ));
    } else {
        println!("Indexed {} files", graph.documents().len());
        println!("Found {} names", graph.declarations().len());
        println!("Found {} definitions", graph.definitions().len());
        println!("Found {} URIs", graph.documents().len());
    }

    // Forget the graph so we don't have to wait for deallocation and let the system reclaim the memory at exit
    mem::forget(graph);
}
