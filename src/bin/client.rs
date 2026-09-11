use maze_wars::client::{DEFAULT_SERVER_ADDRESS, connect, parse_server_address, validate_username};
use maze_wars::window;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Could not start Maze Wars: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let server = ask_for_server()?;
    println!("Using UDP connection to {server}");
    let username = ask_for_username()?;
    println!("Starting...");

    let connection = connect(server, &username)?;
    println!(
        "Connected as player {} on level {}",
        connection.player_id(),
        connection.level()
    );
    window::run(&username, connection)
}

fn ask_for_server() -> Result<SocketAddr, String> {
    let input = prompt(&format!("Enter IP Address [{DEFAULT_SERVER_ADDRESS}]: "))?;
    parse_server_address(&input)
}

fn ask_for_username() -> Result<String, String> {
    loop {
        let input = prompt("Enter Name: ")?;
        match validate_username(&input) {
            Ok(username) => return Ok(username),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not write to the terminal: {error}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("could not read from the terminal: {error}"))?;
    Ok(input.trim().to_owned())
}
