// Copyright (c) 2017-2019 Fabian Schuiki

//! The LLHD reference simulator

#![deny(missing_docs)]

#[macro_use]
extern crate log;

use crate::tracer::Tracer;
use anyhow::{anyhow, Context, Result};
use clap::{App, Arg};
use std::{fs::File, io::prelude::*};

mod builder;
mod engine;
mod state;
pub mod tracer;
pub mod value;

fn main() -> Result<()> {
    // Parse the command line arguments.
    let matches = App::new("llhd-sim")
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(clap::crate_description!())
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Increase message verbosity"),
        )
        .arg(
            Arg::with_name("sequential")
                .short("s")
                .long("sequential")
                .help("Disable parallelization"),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .short("o")
                .long("output")
                .takes_value(true)
                .required(true)
                .help("Trace into an output file"),
        )
        .arg(
            Arg::with_name("INPUT")
                .help("The input file to simulate")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("num-steps")
                .short("s")
                .takes_value(true)
                .help("Terminate after a fixed number of steps"),
        )
        .get_matches();

    // Configure the logger.
    stderrlog::new()
        .quiet(!matches.is_present("verbosity"))
        .verbosity(matches.occurrences_of("verbosity") as usize)
        .init()
        .unwrap();

    // Load the input file.
    let module = {
        // Open the input file.
        let path = matches.value_of("INPUT").unwrap();
        let mut contents = String::new();
        File::open(path)
            .and_then(|mut f| f.read_to_string(&mut contents))
            .with_context(|| format!("failed to read input from {}", path))?;

        // Parse the input file.
        let module = llhd::assembly::parse_module(&contents)
            .map_err(|e| anyhow!("{}", e))
            .with_context(|| format!("failed to parse input from {}", path))?;

        // Verify the file for integrity.
        let mut verifier = llhd::verifier::Verifier::new();
        verifier.verify_module(&module);
        verifier
            .finish()
            .map_err(|e| anyhow!("{}", e))
            .with_context(|| format!("failed to verify input from {}", path))?;

        module
    };

    // Build the simulation state for this module.
    let mut state = builder::build(&module).with_context(|| "failed to initialize simulation")?;

    // Create a new tracer for this state that will generate some waveforms.
    let tracer_path = matches.value_of("OUTPUT").unwrap();
    let mut file = File::create(tracer_path)
        .with_context(|| format!("failed to create output at {}", tracer_path))?;
    let mut tracer: Box<dyn Tracer> = if tracer_path.ends_with(".vcd") {
        Box::new(tracer::VcdTracer::new(&mut file))
    } else if tracer_path.ends_with(".dump") {
        Box::new(tracer::DumpTracer::new(&mut file))
    } else {
        return Err(anyhow!(
            "Cannot determine output format from file name `{}`",
            tracer_path
        ));
    };
    tracer.init(&state);

    // Create the simulation engine and run the simulation to completion.
    {
        let step_limit = matches
            .value_of("num-steps")
            .map(|s| s.parse::<usize>().unwrap());
        let mut engine = engine::Engine::new(&mut state, !matches.is_present("sequential"));
        engine.run(&mut *tracer, step_limit);
    }

    // Flush the tracer.
    tracer.finish(&state);

    Ok(())
}
