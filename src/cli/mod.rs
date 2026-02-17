mod args;
pub use args::*;

pub(crate) mod dashboard;
pub(crate) mod import;
pub(crate) mod internal;
pub(crate) mod server;

use internal::Internal;
use colored::Colorize;
use inquire::Select;
use macros_rs::{crashln, string, ternary};
use pmc::{file, helpers, process::Runner};
use std::env;

pub(crate) fn format(server_name: &String) -> (String, String) {
    let kind = ternary!(
        matches!(&**server_name, "internal" | "local"),
        "",
        "remote "
    )
    .to_string();
    (kind, server_name.to_string())
}

pub fn get_version(short: bool) -> String {
    match short {
        true => format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        false => match env!("GIT_HASH") {
            "" => format!(
                "{} ({}) [{}]",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_DATE"),
                env!("PROFILE")
            ),
            hash => format!(
                "{} ({} {hash}) [{}]",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_DATE"),
                env!("PROFILE")
            ),
        },
    }
}

pub fn start(
    name: &Option<String>,
    args: &Args,
    watch: &Option<String>,
    reset_env: &bool,
    server_name: &String,
) {
    let mut runner = Runner::new();
    let (kind, list_name) = format(server_name);

    let arg = args.get_string().unwrap_or_default();

    if arg == "all" {
        println!(
            "{} Applying {kind}action startAllProcess",
            *helpers::SUCCESS
        );

        let ids: Vec<usize> = runner.items().keys().copied().collect();
        if ids.is_empty() {
            println!("{} Cannot start all, no processes found", *helpers::FAIL);
        } else {
            for id in ids {
                runner = Internal {
                    id,
                    server_name,
                    kind: kind.clone(),
                    runner: runner.clone(),
                }
                .restart(&None, &None, false, true);
            }
        }
    } else {
        match args {
            Args::Id(id) => {
                Internal {
                    id: *id,
                    runner,
                    server_name,
                    kind,
                }
                .restart(name, watch, *reset_env, false);
            }
            Args::Script(script) => match runner.find(script, server_name) {
                Some(id) => {
                    Internal {
                        id,
                        runner,
                        server_name,
                        kind,
                    }
                    .restart(name, watch, *reset_env, false);
                }
                None => {
                    let prefix_matches = runner.find_prefix(script, server_name);
                    match prefix_matches.len() {
                        1 => {
                            let (id, _) = prefix_matches[0].clone();
                            Internal {
                                id,
                                runner,
                                server_name,
                                kind,
                            }
                            .restart(name, watch, *reset_env, false);
                        }
                        n if n > 1 => {
                            println!(
                                "{} Multiple processes match prefix '{script}':",
                                *helpers::FAIL
                            );
                            for (id, proc_name) in &prefix_matches {
                                println!("  {} {id}|{proc_name}", "-".yellow());
                            }
                        }
                        _ => {
                            Internal {
                                id: 0,
                                runner,
                                server_name,
                                kind,
                            }
                            .create(script, name, watch, false);
                        }
                    }
                }
            },
        }
    }

    Internal::list(&string!("default"), &list_name);
}

pub fn stop(item: &Item, server_name: &String) {
    let mut runner: Runner = Runner::new();
    let (kind, list_name) = format(server_name);

    let arg = item.get_string().unwrap_or_default();

    if arg == "all" {
        println!("{} Applying {kind}action stopAllProcess", *helpers::SUCCESS);

        let ids: Vec<usize> = runner.items().keys().copied().collect();
        if ids.is_empty() {
            println!("{} Cannot stop all, no processes found", *helpers::FAIL);
        } else {
            for id in ids {
                runner = Internal {
                    id,
                    server_name,
                    kind: kind.clone(),
                    runner: runner.clone(),
                }
                .stop(true);
            }
        }
    } else {
        match item {
            Item::Id(id) => {
                Internal {
                    id: *id,
                    runner,
                    server_name,
                    kind,
                }
                .stop(false);
            }
            Item::Name(name) => match runner.find(name, server_name) {
                Some(id) => {
                    Internal {
                        id,
                        runner,
                        server_name,
                        kind,
                    }
                    .stop(false);
                }
                None => crashln!("{} Process ({name}) not found", *helpers::FAIL),
            },
        }
    }

    Internal::list(&string!("default"), &list_name);
}

pub fn remove(item: &Item, server_name: &String) {
    let runner: Runner = Runner::new();
    let (kind, _) = format(server_name);

    let arg = item.get_string().unwrap_or_default();

    if arg == "all" {
        println!(
            "{} Applying {kind}action removeAllProcess",
            *helpers::SUCCESS
        );

        let ids: Vec<usize> = runner.items().keys().copied().collect();
        if ids.is_empty() {
            println!("{} Cannot remove all, no processes found", *helpers::FAIL);
        } else {
            for id in ids {
                Internal {
                    id,
                    server_name,
                    kind: kind.clone(),
                    runner: Runner::new(),
                }
                .remove();
            }
        }
    } else {
        match item {
            Item::Id(id) => Internal {
                id: *id,
                runner,
                server_name,
                kind,
            }
            .remove(),
            Item::Name(name) => match runner.find(name, server_name) {
                Some(id) => Internal {
                    id,
                    runner,
                    server_name,
                    kind,
                }
                .remove(),
                None => crashln!("{} Process ({name}) not found", *helpers::FAIL),
            },
        }
    }

    super::daemon::reset();
}

pub fn info(item: &Item, format: &String, server_name: &String) {
    let runner: Runner = Runner::new();
    let (kind, _) = self::format(server_name);

    match item {
        Item::Id(id) => Internal {
            id: *id,
            runner,
            server_name,
            kind,
        }
        .info(format),
        Item::Name(name) => match runner.find(name, server_name) {
            Some(id) => Internal {
                id,
                runner,
                server_name,
                kind,
            }
            .info(format),
            None => crashln!("{} Process ({name}) not found", *helpers::FAIL),
        },
    }
}

pub fn logs(item: &Item, lines: &usize, server_name: &String) {
    let runner: Runner = Runner::new();
    let (kind, _) = format(server_name);

    let arg = item.get_string().unwrap_or_default();

    if arg == "all" {
        if runner.is_empty() {
            println!("{} Cannot show logs, no processes found", *helpers::FAIL);
            return;
        }

        for (id, process) in runner.items() {
            println!(
                "{}",
                format!(
                    "\nShowing last {lines} lines for {kind}process [{id}] ({}):",
                    process.name
                )
                .yellow()
            );
            file::logs(&process, *lines, "error");
            file::logs(&process, *lines, "out");
        }
        return;
    }

    match item {
        Item::Id(id) => Internal {
            id: *id,
            runner,
            server_name,
            kind,
        }
        .logs(lines),
        Item::Name(name) => match runner.find(name, server_name) {
            Some(id) => Internal {
                id,
                runner,
                server_name,
                kind,
            }
            .logs(lines),
            None => {
                let matches = runner.find_partial(name, server_name);
                if matches.is_empty() {
                    crashln!("{} Process ({name}) not found", *helpers::FAIL);
                }

                println!(
                    "{} Process ({name}) not found, matched by pattern:",
                    *helpers::FAIL
                );

                let options: Vec<String> = matches
                    .iter()
                    .map(|(id, proc_name)| format!("{id}|{proc_name}"))
                    .collect();

                match Select::new("Show logs for?", options).prompt() {
                    Ok(selected) => {
                        let id: usize = selected
                            .split('|')
                            .next()
                            .unwrap()
                            .parse()
                            .unwrap();

                        Internal {
                            id,
                            runner,
                            server_name,
                            kind,
                        }
                        .logs(lines);
                    }
                    Err(_) => crashln!("{} Selection cancelled", *helpers::FAIL),
                }
            }
        },
    }
}

pub fn details(lines: &usize, server_name: &String) {
    Internal::details(lines, server_name);
}

// combine into a single function that handles multiple
pub fn env(item: &Item, server_name: &String) {
    let runner: Runner = Runner::new();
    let (kind, _) = format(server_name);

    match item {
        Item::Id(id) => Internal {
            id: *id,
            runner,
            server_name,
            kind,
        }
        .env(),
        Item::Name(name) => match runner.find(name, server_name) {
            Some(id) => Internal {
                id,
                runner,
                server_name,
                kind,
            }
            .env(),
            None => crashln!("{} Process ({name}) not found", *helpers::FAIL),
        },
    }
}

pub fn flush(item: &Item, server_name: &String) {
    let runner: Runner = Runner::new();
    let (kind, _) = format(server_name);

    let arg = item.get_string().unwrap_or_default();

    if arg == "all" {
        println!(
            "{} Applying {kind}action flushAllProcess",
            *helpers::SUCCESS
        );

        let ids: Vec<usize> = runner.items().keys().copied().collect();
        if ids.is_empty() {
            println!("{} Cannot flush all, no processes found", *helpers::FAIL);
        } else {
            for id in ids {
                Internal {
                    id,
                    server_name,
                    kind: kind.clone(),
                    runner: Runner::new(),
                }
                .flush();
            }
        }
        return;
    }

    match item {
        Item::Id(id) => Internal {
            id: *id,
            runner,
            server_name,
            kind,
        }
        .flush(),
        Item::Name(name) => match runner.find(name, server_name) {
            Some(id) => Internal {
                id,
                runner,
                server_name,
                kind,
            }
            .flush(),
            None => crashln!("{} Process ({name}) not found", *helpers::FAIL),
        },
    }
}
