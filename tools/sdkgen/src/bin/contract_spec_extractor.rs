use sdkgen::{run, Config};
use std::{env, path::PathBuf, process};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args).and_then(run) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("contract_spec_extractor: {err}");
            process::exit(1);
        }
    }
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut wasm = None;
    let mut spec_json = None;
    let mut typescript = None;
    let mut python = None;
    let mut check = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--wasm" => {
                i += 1;
                wasm = args.get(i).map(PathBuf::from);
            }
            "--spec-json" => {
                i += 1;
                spec_json = args.get(i).map(PathBuf::from);
            }
            "--typescript" => {
                i += 1;
                typescript = args.get(i).map(PathBuf::from);
            }
            "--python" => {
                i += 1;
                python = args.get(i).map(PathBuf::from);
            }
            "--check" => check = true,
            "--help" | "-h" => return Err(help()),
            unknown => return Err(format!("unknown argument `{unknown}`\n\n{}", help())),
        }
        i += 1;
    }

    Ok(Config {
        wasm: wasm.ok_or_else(help)?,
        spec_json,
        typescript,
        python,
        check,
    })
}

fn help() -> String {
    "usage: contract_spec_extractor --wasm <contract.wasm> [--spec-json <out.json>] [--typescript <client.ts>] [--python <client.py>] [--check]".to_string()
}
